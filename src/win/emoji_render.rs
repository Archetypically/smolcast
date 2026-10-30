//! Color emoji rasterization with Windows' own text stack (plan §9, revised).
//!
//! egui's bundled monochrome font covers only part of Unicode Emoji and has no
//! text shaping, so ZWJ, flag, and keycap sequences fall apart. Instead, one
//! worker thread lays out each requested sequence in Segoe UI Emoji with
//! DirectWrite and draws it with Direct2D into a software WIC bitmap with
//! color fonts enabled. The result is converted to owned RGBA on that thread;
//! no COM object leaves it. The UI requests only visible cells and caches a
//! bounded number of textures.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1CreateFactory, ID2D1Factory,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory,
    IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppPBGRA, IWICImagingFactory,
    WICBitmapCacheOnLoad,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::w;

use crate::app::model::{AppEvent, Wake};
use crate::win::icon::IconRgba;
use crate::win::join_by;

/// Glyph em size relative to the square bitmap edge.
const EM_FRACTION: f32 = 0.78;

struct Request {
    index: usize,
    text: &'static str,
    edge: u32,
}

/// The rasterizer thread. Results arrive as `AppEvent::EmojiGlyph`.
pub struct EmojiRenderer {
    tx: Option<Sender<Request>>,
    cancel: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl EmojiRenderer {
    pub fn start(wake: Wake) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let join = thread::Builder::new()
            .name("smolcast-emoji".into())
            .spawn(move || render_main(rx, worker_cancel, wake))?;
        Ok(Self {
            tx: Some(tx),
            cancel,
            join: Some(join),
        })
    }

    /// Render `text` (an entry of the emoji table) into an `edge`-pixel square.
    pub fn request(&self, index: usize, text: &'static str, edge: u32) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Request { index, text, edge });
        }
    }

    /// Drop queued work and stop, waiting for the thread until `deadline`
    /// (§1.5 step 5).
    pub fn shutdown(mut self, deadline: Instant) {
        self.stop_and_join(Some(deadline));
    }

    fn stop_and_join(&mut self, deadline: Option<Instant>) {
        self.cancel.store(true, Ordering::Release);
        self.tx = None;
        if let Some(join) = self.join.take() {
            join_by(join, deadline);
        }
    }
}

impl Drop for EmojiRenderer {
    fn drop(&mut self) {
        self.stop_and_join(None);
    }
}

/// Glyph edge used to warm the renderer up; the font load is size-independent.
const WARM_EDGE: u32 = 32;

/// Thread-local drawing objects, created once.
struct Stack {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    wic: IWICImagingFactory,
    /// Text format for the most recent edge size.
    format: Option<(u32, IDWriteTextFormat)>,
}

impl Stack {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            Ok(Self {
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                dwrite: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
                wic: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                format: None,
            })
        }
    }

    fn format(&mut self, edge: u32) -> windows::core::Result<IDWriteTextFormat> {
        if let Some((cached, format)) = &self.format
            && *cached == edge
        {
            return Ok(format.clone());
        }
        let format = unsafe {
            let format = self.dwrite.CreateTextFormat(
                w!("Segoe UI Emoji"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                edge as f32 * EM_FRACTION,
                w!("en-us"),
            )?;
            format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            format
        };
        self.format = Some((edge, format.clone()));
        Ok(format)
    }

    fn render(&mut self, text: &str, edge: u32) -> windows::core::Result<IconRgba> {
        let format = self.format(edge)?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        unsafe {
            let bitmap = self.wic.CreateBitmap(
                edge,
                edge,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnLoad,
            )?;
            let target = self.d2d.CreateWicBitmapRenderTarget(
                &bitmap,
                &D2D1_RENDER_TARGET_PROPERTIES {
                    r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                    usage: D2D1_RENDER_TARGET_USAGE_NONE,
                    minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
                },
            )?;
            // The brush colors only glyphs without color layers. White, so
            // the picker can tint text symbols to the theme (emoji_view).
            let brush = target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                },
                None,
            )?;
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F::default()));
            let rect = D2D_RECT_F {
                left: 0.0,
                top: 0.0,
                right: edge as f32,
                bottom: edge as f32,
            };
            target.DrawText(
                &utf16,
                &format,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            target.EndDraw(None, None)?;

            let stride = edge * 4;
            let mut pixels = vec![0u8; (stride * edge) as usize];
            bitmap.CopyPixels(std::ptr::null(), stride, &mut pixels)?;
            for px in pixels.as_chunks_mut::<4>().0 {
                px.swap(0, 2); // BGRA -> RGBA, still premultiplied
            }
            Ok(IconRgba {
                width: edge,
                height: edge,
                rgba_premultiplied: pixels,
            })
        }
    }
}

fn render_main(rx: Receiver<Request>, cancel: Arc<AtomicBool>, wake: Wake) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok();
    let mut stack = com.as_ref().ok().map(|_| Stack::new());
    // Warm up before the picker first opens: search keys, and the emoji font
    // that DirectWrite otherwise loads for the first visible glyph.
    crate::core::emoji::prepare();
    if let Some(Ok(stack)) = &mut stack {
        let _ = stack.render("\u{1F600}", WARM_EDGE);
    }
    while let Ok(request) = rx.recv() {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let result = match &mut stack {
            Some(Ok(stack)) => stack
                .render(request.text, request.edge)
                .map_err(|e| e.message()),
            Some(Err(e)) => Err(e.message()),
            None => Err("COM initialization failed".into()),
        };
        wake.send(AppEvent::EmojiGlyph {
            index: request.index,
            edge: request.edge,
            result,
        });
    }
    // Release DirectWrite/Direct2D/WIC objects before leaving the apartment.
    drop(stack);
    if com.is_ok() {
        unsafe { CoUninitialize() };
    }
}
