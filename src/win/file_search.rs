//! File search through the Windows Search index (plan §6.6).
//!
//! One long-lived STA thread holds an OLE DB session with the Windows Search
//! provider (`Search.CollatorDSO`) and runs the SQL from `core::file_search`.
//! Typing queues a query per keystroke; the worker runs only the newest one,
//! so a burst of keys costs one query. Results reach the UI as
//! `AppEvent::FilesFound`. smolcast keeps no index of its own: what the
//! Windows Search service has not indexed is not found.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Search::{
    ICommandText, IDBCreateCommand, IDBCreateSession, IDBInitialize, IGetRow, IRowset,
};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::core::{GUID, IUnknown, Interface, PCWSTR};

use crate::app::model::{AppEvent, Wake};
use crate::core::file_search::{self, FileHit};
use crate::win::shell::ComApartment;
use crate::win::{join_by, take_pwstr, wide};

/// `CLSID_CollatorDataSource` (searchapi.h): the Windows Search OLE DB
/// provider.
const CLSID_COLLATOR_DATA_SOURCE: GUID = GUID::from_u128(0x9e175b8b_f52a_11d8_b9a5_505054503030);
/// `DBGUID_DEFAULT` (oledb.h): the provider's own SQL dialect.
const DBGUID_DEFAULT: GUID = GUID::from_u128(0xc8b521fb_5cf3_11ce_ade5_00aa0044773d);
/// `System.ItemPathDisplay` ({E3E0584C-B788-4A5A-BB20-7F5A44C9ACDD}, 7).
const PKEY_ITEM_PATH_DISPLAY: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xe3e0584c_b788_4a5a_bb20_7f5a44c9acdd),
    pid: 7,
};

struct Request {
    generation: u64,
    query: String,
}

/// Owns the search thread; dropping it stops and joins the thread.
pub struct FileSearchWorker {
    tx: Option<Sender<Request>>,
    join: Option<JoinHandle<()>>,
}

impl FileSearchWorker {
    pub fn start(wake: Wake) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("smolcast-file-search".into())
            .spawn(move || search_main(rx, wake))?;
        Ok(Self {
            tx: Some(tx),
            join: Some(join),
        })
    }

    pub fn request(&self, generation: u64, query: String) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Request { generation, query });
        }
    }

    /// Stop, waiting for the thread until `deadline` (plan §1.5).
    pub fn shutdown(mut self, deadline: Instant) {
        self.stop_and_join(Some(deadline));
    }

    fn stop_and_join(&mut self, deadline: Option<Instant>) {
        // Closing the channel ends the loop after the query in flight.
        self.tx = None;
        if let Some(join) = self.join.take() {
            join_by(join, deadline);
        }
    }
}

impl Drop for FileSearchWorker {
    fn drop(&mut self) {
        self.stop_and_join(None);
    }
}

fn search_main(rx: Receiver<Request>, wake: Wake) {
    let com = ComApartment::enter();
    // Connected on the first query, and again after a failure (the
    // Windows Search service may have been restarted).
    let mut session: Option<IDBCreateCommand> = None;
    while let Ok(mut request) = rx.recv() {
        // Only the newest query matters; older ones are already stale.
        while let Ok(newer) = rx.try_recv() {
            request = newer;
        }
        let result = match &com {
            Ok(_) => search(&mut session, &request.query),
            Err(e) => Err(e.to_string()),
        };
        if result.is_err() {
            session = None;
        }
        wake.send(AppEvent::FilesFound {
            generation: request.generation,
            result,
        });
    }
}

fn search(session: &mut Option<IDBCreateCommand>, query: &str) -> Result<Vec<FileHit>, String> {
    let Some(sql) = file_search::sql(query) else {
        return Ok(Vec::new());
    };
    let commands = match session {
        Some(commands) => commands.clone(),
        None => {
            let commands = connect().map_err(|e| {
                format!(
                    "Windows Search is not available ({}). Check that the Windows Search \
                     service is running.",
                    e.message()
                )
            })?;
            session.insert(commands).clone()
        }
    };
    let mut hits =
        run(&commands, &sql).map_err(|e| format!("File search failed: {}", e.message()))?;
    file_search::rerank(&mut hits, query);
    Ok(hits)
}

fn connect() -> windows::core::Result<IDBCreateCommand> {
    let init: IDBInitialize =
        unsafe { CoCreateInstance(&CLSID_COLLATOR_DATA_SOURCE, None, CLSCTX_INPROC_SERVER) }?;
    unsafe { init.Initialize() }?;
    let sessions: IDBCreateSession = init.cast()?;
    unsafe { sessions.CreateSession(None, &IDBCreateCommand::IID) }?.cast()
}

fn run(commands: &IDBCreateCommand, sql: &str) -> windows::core::Result<Vec<FileHit>> {
    let command: ICommandText =
        unsafe { commands.CreateCommand(None, &ICommandText::IID) }?.cast()?;
    let text = wide(sql);
    unsafe { command.SetCommandText(&DBGUID_DEFAULT, PCWSTR(text.as_ptr())) }?;
    let mut rowset: Option<IUnknown> = None;
    unsafe { command.Execute(None, &IRowset::IID, None, None, Some(&mut rowset)) }?;
    let Some(rowset) = rowset else {
        return Ok(Vec::new());
    };
    let rowset: IRowset = rowset.cast()?;
    let rows: IGetRow = rowset.cast()?;
    let mut hits = Vec::new();
    while hits.len() < file_search::MAX_RESULTS {
        // One row per call: the generated binding passes the slice as the
        // provider's `HROW**`, so a one-element slice is exactly "fill this".
        let mut row: usize = 0;
        let mut obtained: usize = 0;
        let mut slot = [&mut row as *mut usize];
        unsafe { rowset.GetNextRows(0, 0, &mut obtained, &mut slot) }?;
        if obtained == 0 {
            break; // DB_S_ENDOFROWSET
        }
        let path = read_path(&rows, row);
        let _ = unsafe {
            rowset.ReleaseRows(
                1,
                &row,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // The index can lag behind deletions: skip what no longer exists.
        if let Some(path) = path
            && let Ok(meta) = std::fs::metadata(&path)
        {
            hits.push(FileHit::from_path(path, meta.is_dir()));
        }
    }
    Ok(hits)
}

fn read_path(rows: &IGetRow, row: usize) -> Option<String> {
    let store: IPropertyStore = unsafe { rows.GetRowFromHROW(None, row, &IPropertyStore::IID) }
        .ok()?
        .cast()
        .ok()?;
    let value = unsafe { store.GetValue(&PKEY_ITEM_PATH_DISPLAY) }.ok()?;
    let path = unsafe { PropVariantToStringAlloc(&value) }
        .map(take_pwstr)
        .ok()?;
    (!path.is_empty()).then_some(path)
}
