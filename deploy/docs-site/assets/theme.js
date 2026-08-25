/* CLOISON — docs.wonkom.ai — bascule de thème (sans stockage, suit l'OS par défaut). */
(function () {
  var themeBtn = document.getElementById('themeBtn');
  if (!themeBtn) return;
  function currentTheme() {
    var t = document.documentElement.getAttribute('data-theme');
    if (t === 'dark' || t === 'light') return t;
    return window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  }
  themeBtn.addEventListener('click', function () {
    var root = document.documentElement;
    var next = currentTheme() === 'dark' ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    themeBtn.setAttribute('aria-pressed', next === 'dark');
  });
})();
