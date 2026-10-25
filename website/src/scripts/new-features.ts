/* Plays each What's New scene while its card is on screen: steps appear one by one
 * (typed when marked data-type), transient ones (a typing indicator) vanish again, and
 * the scene loops after a pause. Reduced motion shows every scene finished and still. */
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
const PAUSE = 2800;

interface Scene { card: HTMLElement; steps: HTMLElement[]; run: number; visible: boolean }

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

async function type(el: HTMLElement, run: () => boolean): Promise<void> {
  const text = el.dataset.text ?? (el.dataset.text = el.textContent ?? '');
  el.textContent = '';
  el.classList.add('on');
  for (const ch of text) {
    if (!run()) { el.textContent = text; return; }
    el.textContent += ch;
    await sleep(70);
  }
}

function level(card: HTMLElement, step: HTMLElement): void {
  if (step.dataset.meter === undefined) return;
  const saver = card.querySelector<HTMLElement>('.nf-saver');
  if (!saver) return;
  const levels = [...card.querySelectorAll<HTMLElement>('[data-meter]')];
  saver.dataset.level = String(levels.indexOf(step));
  saver.style.setProperty('--meter', step.dataset.meter);
  const count = card.querySelector('.nf-count');
  if (count) count.textContent = `≈ ${step.dataset.count} tokens`;
}

function reset(scene: Scene): void {
  scene.steps.forEach(step => { step.classList.remove('on'); if (step.dataset.text !== undefined) step.textContent = step.dataset.text; });
  const first = scene.card.querySelector<HTMLElement>('[data-meter]');
  if (first) level(scene.card, first);
}

async function play(scene: Scene): Promise<void> {
  const id = ++scene.run;
  const live = () => scene.visible && scene.run === id && !reduced.matches;
  scene.card.classList.add('is-playing');
  while (live()) {
    reset(scene);
    let transient: HTMLElement | null = null;
    for (const step of scene.steps) {
      await sleep(Number(step.dataset.wait ?? 700));
      if (!live()) return;
      transient?.classList.remove('on');
      transient = null;
      level(scene.card, step);
      if (step.hasAttribute('data-type')) await type(step, live);
      else step.classList.add('on');
      if (step.hasAttribute('data-transient')) transient = step;
    }
    await sleep(PAUSE);
  }
}

function still(scene: Scene): void {
  scene.run++;
  scene.card.classList.remove('is-playing');
  scene.card.classList.add('is-still');
}

export function initNewFeatures(): void {
  const scenes = [...document.querySelectorAll<HTMLElement>('.nf-card[data-nf]')].map(card => ({ card, steps: [...card.querySelectorAll<HTMLElement>('[data-step]')], run: 0, visible: false }));
  if (!scenes.length) return;
  if (reduced.matches) { scenes.forEach(still); return; }
  const observer = new IntersectionObserver(entries => entries.forEach(entry => {
    const scene = scenes.find(item => item.card === entry.target)!;
    scene.visible = entry.isIntersecting;
    if (scene.visible) void play(scene);
    else scene.run++;
  }), { threshold: .35 });
  scenes.forEach(scene => observer.observe(scene.card));
  reduced.addEventListener('change', () => { if (reduced.matches) scenes.forEach(still); });
  // A soft light follows the pointer across each card.
  document.querySelectorAll<HTMLElement>('.nf-card').forEach(card => card.addEventListener('pointermove', event => {
    const rect = card.getBoundingClientRect();
    card.style.setProperty('--mx', `${event.clientX - rect.left}px`);
    card.style.setProperty('--my', `${event.clientY - rect.top}px`);
  }));
}
