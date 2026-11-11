// Sets the colour theme before the first paint so the page doesn't flash the wrong one. It is a
// separate file, not an inline script, because the dashboard runs under `script-src 'self'`.
(function () {
  var theme = localStorage.getItem('app.themeMode');
  if (theme === 'light' || theme === 'dark') {
    document.documentElement.setAttribute('data-theme', theme);
  } else {
    var dark = window.matchMedia('(prefers-color-scheme: dark)').matches;
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light');
  }
})();
