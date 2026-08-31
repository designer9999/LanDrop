// Pre-paint theme boot: runs as a blocking classic script (CSP: 'self') so the
// very first frame already matches the persisted theme — no white flash in
// dark mode, no opaque flash when mica transparency is enabled. The full token
// set is applied by the app bundle right after mount; this only seeds
// color-scheme plus the cached surface pair written by applyThemeToDOM.
(function () {
  try {
    var cfg = JSON.parse(localStorage.getItem("landrop-theme") || "{}");
    var boot = JSON.parse(localStorage.getItem("landrop-theme-boot") || "{}");
    var isDark = cfg.isDark !== false; // app default is dark
    var root = document.documentElement;
    var surface = boot.surface || (isDark ? "#141218" : "#fef7ff");
    var onSurface = boot.onSurface || (isDark ? "#e6e0e9" : "#1d1b20");
    root.style.colorScheme = isDark ? "dark" : "light";
    root.style.setProperty("--md-sys-color-surface", surface);
    root.style.setProperty("--md-sys-color-on-surface", onSurface);
    root.style.background = cfg.mica && isDark ? "transparent" : surface;
  } catch (e) {
    // First run or blocked storage — render bundle defaults.
  }
})();
