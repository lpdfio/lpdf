const THEME_ICON = `<svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" fill="currentColor" viewBox="0 0 256 256"><path d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm8,16.37a86.4,86.4,0,0,1,16,3V212.67a86.4,86.4,0,0,1-16,3Zm32,9.26a87.81,87.81,0,0,1,16,10.54V195.83a87.81,87.81,0,0,1-16,10.54ZM40,128a88.11,88.11,0,0,1,80-87.63V215.63A88.11,88.11,0,0,1,40,128Zm160,50.54V77.46a87.82,87.82,0,0,1,0,101.08Z"></path></svg>`;
const SUN  = THEME_ICON;
const MOON = THEME_ICON;

function toggleTheme() {
  const next = document.documentElement.getAttribute('data-theme') === 'dark' ? 'light' : 'dark';
  document.documentElement.setAttribute('data-theme', next);
  try { localStorage.setItem('lpdf-theme', next); } catch(e) {}
  updateIcon();
}

function updateIcon() {
  const btn = document.getElementById('theme-btn');
  if (!btn) return;
  const isDark = document.documentElement.getAttribute('data-theme') === 'dark';
  btn.innerHTML = isDark ? SUN : MOON;
}

// Run on page load and after any html-include partial is injected
updateIcon();
document.addEventListener('html-include:loaded', updateIcon);

function markActiveNavLink() {
    const path = window.location.pathname.replace(/\/$/, '') || '/';
    document.querySelectorAll('.nav-links a').forEach(a => {
        const href = a.getAttribute('href').replace(/\/$/, '') || '/';
        a.classList.toggle('active', href === path);
    });
}
markActiveNavLink();
document.addEventListener('html-include:loaded', markActiveNavLink);
