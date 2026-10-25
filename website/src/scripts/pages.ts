/* Reading pages: the navigation's progress line, reveals as releases and sections scroll
 * in, and a contents rail (changelog versions, legal sections) that follows the reader.
 * Content is visible until this script marks the page `motion-ready`. */
const root = document.documentElement;
const nav = document.querySelector<HTMLElement>('.navigation');
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
let frame = 0;

function progress(): void {
  frame = 0;
  const max = root.scrollHeight - innerHeight;
  nav?.style.setProperty('--progress', String(max > 0 ? Math.min(1, scrollY / max) : 0));
}
addEventListener('scroll', () => { if (!frame) frame = requestAnimationFrame(progress); }, { passive: true });
progress();

const revealed = new IntersectionObserver(entries => entries.forEach(entry => {
  if (!entry.isIntersecting) return;
  entry.target.classList.add('is-visible');
  revealed.unobserve(entry.target);
}), { threshold: .08 });
const targets = document.querySelectorAll('.release, .legal-prose section');
if (!reduced.matches) {
  // Anything already above the fold (or a deep link's target) shows at once.
  targets.forEach(el => { if (el.getBoundingClientRect().top < innerHeight) el.classList.add('is-visible'); });
  root.classList.add('motion-ready');
}
targets.forEach(el => revealed.observe(el));

/** Marks the rail link whose section sits nearest the top third of the screen. */
function followRail(links: readonly HTMLAnchorElement[], sections: readonly HTMLElement[], rail: HTMLElement): void {
  let current = '';
  const mark = () => {
    const line = innerHeight * .33;
    const active = sections.reduce<HTMLElement | undefined>((found, section) => section.getBoundingClientRect().top <= line ? section : found, undefined) ?? sections[0];
    if (!active || active.id === current) return;
    current = active.id;
    links.forEach(link => {
      const on = link.hash === `#${current}`;
      link.classList.toggle('is-current', on);
      if (on) link.setAttribute('aria-current', 'location'); else link.removeAttribute('aria-current');
      if (on && rail.scrollHeight > rail.clientHeight + 4) rail.scrollTo({ top: link.offsetTop - rail.clientHeight / 2, behavior: 'smooth' });
      if (on && rail.scrollWidth > rail.clientWidth + 4) rail.scrollTo({ left: link.offsetLeft - rail.clientWidth / 2, behavior: 'smooth' });
    });
  };
  let pending = 0;
  addEventListener('scroll', () => { if (!pending) pending = requestAnimationFrame(() => { pending = 0; mark(); }); }, { passive: true });
  mark();
}

const rail = document.querySelector<HTMLElement>('.changelog-rail, .legal-toc');
if (rail) {
  const links = [...rail.querySelectorAll<HTMLAnchorElement>('a[href^="#"]')];
  const sections = links.map(link => document.getElementById(link.hash.slice(1))).filter((el): el is HTMLElement => el !== null);
  followRail(links, sections, rail);
}
