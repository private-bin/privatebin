// Loaded as a blocking classic script in <head> so the theme applies before first paint.
(() => {
  const saved = (): string | null => {
    try {
      return localStorage.getItem('pb-theme');
    } catch {
      return null; // storage unavailable (private mode): follow the system preference
    }
  };
  const media = window.matchMedia('(prefers-color-scheme: dark)');
  const apply = () => {
    const choice = saved();
    document.documentElement.dataset.theme = choice === 'dark' || choice === 'light' ? choice : media.matches ? 'dark' : 'light';
  };
  apply();
  // Re-read the choice on every change: the user may have picked a theme since load.
  media.addEventListener('change', apply);
})();
