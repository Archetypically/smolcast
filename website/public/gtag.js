window.dataLayer = window.dataLayer || [];
function gtag() {
  window.dataLayer.push(arguments);
}
gtag("js", new Date());
// Visit counts only: no Google Signals or ad personalization, whose
// doubleclick and ga-audiences beacons the CSP blocks (privacy page).
gtag("config", "G-REY2NJ2MYB", {
  allow_google_signals: false,
  allow_ad_personalization_signals: false,
});
