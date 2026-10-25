/* Scroll-linked motion for the landing page: the hero landscape parallaxes, the hero
 * preview straightens as it scrolls up, a progress line fills the navigation, and
 * groups (harnesses, What's New cards) arrive together when they come into view.
 * Scroll is only observed, never intercepted; reduced motion skips it all. */
import { initNewFeatures } from './new-features';

const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const root = document.documentElement;
const hero = document.querySelector<HTMLElement>('.hero');
const landscape = document.querySelector<HTMLElement>('.hero-landscape');
const preview = document.querySelector<HTMLElement>('#hero-demo > .app-window');
const nav = document.querySelector<HTMLElement>('.navigation');
let frame = 0;

function update(): void {
  frame = 0;
  const y = scrollY;
  const max = root.scrollHeight - innerHeight;
  nav?.style.setProperty('--progress', String(max > 0 ? Math.min(1, y / max) : 0));
  if (reduced.matches || !hero) return;
  const height = hero.offsetHeight;
  if (y > height) return;
  landscape?.style.setProperty('--parallax', `${(y * .28).toFixed(1)}px`);
  if (preview) {
    // Tilted back at the top, flat once it reaches the middle of the screen.
    const rect = preview.getBoundingClientRect();
    const progress = Math.min(1, Math.max(0, 1 - (rect.top - innerHeight * .25) / (innerHeight * .6)));
    preview.style.setProperty('--tilt', `${(14 * (1 - progress)).toFixed(2)}deg`);
    preview.style.setProperty('--tilt-scale', (.96 + .04 * progress).toFixed(3));
  }
}
const schedule = () => { if (!frame) frame = requestAnimationFrame(update); };

const groups = new IntersectionObserver(entries => entries.forEach(entry => {
  if (!entry.isIntersecting) return;
  entry.target.classList.add('is-visible');
  groups.unobserve(entry.target);
}), { threshold: .12 });
document.querySelectorAll('.harness-list, .nf-grid').forEach(el => groups.observe(el));

addEventListener('scroll', schedule, { passive: true });
addEventListener('resize', schedule);
update();
initNewFeatures();
