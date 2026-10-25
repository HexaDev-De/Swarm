const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
const navigationObserver = new IntersectionObserver(([entry]) => {
  document.querySelector('.navigation').classList.toggle('is-scrolled', !entry.isIntersecting);
}, {rootMargin: '-76px 0px 0px'});
navigationObserver.observe(document.querySelector('.hero-copy'));
const story = document.querySelector('#story-demo');
const chapterElements = [...document.querySelectorAll('.story-step')];
let currentScene = 'conversation';
let manualUntil = 0;

function setScene(scene) {
  if (!['conversation', 'board', 'team', 'review'].includes(scene)) return;
  currentScene = scene;
  story.querySelector('.app-window').dataset.currentScene = scene;
  story.querySelectorAll('[data-preview-scene]').forEach(el => { el.hidden = el.dataset.previewScene !== scene; });
  story.querySelector('.app-view-switch').textContent = { conversation: 'Conversation', board: 'Board', team: 'Team', review: 'Changes' }[scene];
  document.querySelectorAll('[data-show-scene]').forEach(button => button.setAttribute('aria-pressed', String(button.dataset.showScene === scene)));
  chapterElements.forEach(el => el.classList.toggle('is-current', el.dataset.scene === scene));
  story.querySelectorAll('.model-menu').forEach(menu => { menu.hidden = true; });
  story.querySelectorAll('.app-model').forEach(button => button.setAttribute('aria-expanded', 'false'));
}

document.querySelectorAll('[data-show-scene]').forEach(button => button.addEventListener('click', () => {
  // A manual choice stays put until the next intentional scroll.
  manualUntil = Infinity;
  setScene(button.dataset.showScene);
}));
const resumeScroll = () => { manualUntil = 0; };
window.addEventListener('wheel', resumeScroll, {passive: true});
window.addEventListener('touchmove', resumeScroll, {passive: true});
window.addEventListener('keydown', event => {
  if (['ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', 'Home', 'End'].includes(event.key) || (event.key === ' ' && !event.target.closest('button'))) resumeScroll();
});
window.addEventListener('pointerdown', event => {
  if (event.clientX >= document.documentElement.clientWidth - 16 || event.target.closest('a[href^="#"]')) resumeScroll();
}, {passive: true});

// Scroll is observed, never intercepted. One short frame only when scrolling.
let scrollFrame = 0;
function updateChapter() {
  scrollFrame = 0;
  if (performance.now() < manualUntil || !chapterElements.length) return;
  const line = innerHeight * (innerWidth <= 760 ? .80 : .5);
  const nearest = chapterElements.reduce((best, el) => {
    const rect = el.getBoundingClientRect();
    const distance = Math.abs(rect.top + rect.height / 2 - line);
    return distance < best.distance ? { el, distance } : best;
  }, { el: chapterElements[0], distance: Infinity }).el;
  if (nearest.dataset.scene !== currentScene) setScene(nearest.dataset.scene);
}
const storyObserver = new IntersectionObserver(([entry]) => {
  if (entry.isIntersecting) { window.addEventListener('scroll', onScroll, { passive: true }); updateChapter(); }
  else { window.removeEventListener('scroll', onScroll); cancelAnimationFrame(scrollFrame); scrollFrame = 0; }
});
function onScroll() { if (!scrollFrame) scrollFrame = requestAnimationFrame(updateChapter); }
storyObserver.observe(document.querySelector('.story-layout'));

const revealObserver = new IntersectionObserver(entries => entries.forEach(entry => {
  if (entry.isIntersecting) { entry.target.classList.add('is-visible'); revealObserver.unobserve(entry.target); }
}), { threshold: .08 });
document.documentElement.classList.add('motion-ready');
document.querySelectorAll('.reveal').forEach(el => revealObserver.observe(el));

const providerLabels = {claude: 'Claude Code', codex: 'Codex', kimi: 'Kimi Code', opencode: 'OpenCode'};
document.querySelectorAll('.app-model').forEach(button => button.addEventListener('click', () => {
  const menu = button.closest('.app-main').querySelector('.model-menu');
  menu.hidden = !menu.hidden;
  button.setAttribute('aria-expanded', String(!menu.hidden));
  if (!menu.hidden) menu.querySelector('[aria-pressed=true]').focus({preventScroll: true});
}));
document.querySelectorAll('[data-model]').forEach(button => button.addEventListener('click', () => {
  const main = button.closest('.app-main');
  const trigger = main.querySelector('.app-model');
  trigger.querySelector('img').replaceWith(button.querySelector('img').cloneNode());
  trigger.querySelector('span').textContent = button.textContent.trim();
  main.querySelector('.response-provider').textContent = providerLabels[button.dataset.model];
  main.querySelector('.app-message > img').replaceWith(button.querySelector('img').cloneNode());
  main.querySelectorAll('[data-model]').forEach(option => option.setAttribute('aria-pressed', String(option === button)));
  main.querySelector('.model-menu').hidden = true;
  trigger.setAttribute('aria-expanded', 'false');
  trigger.focus({preventScroll: true});
}));
document.addEventListener('pointerdown', event => {
  if (!event.target.closest('.model-menu, .app-model')) document.querySelectorAll('.model-menu').forEach(menu => {
    menu.hidden = true;
    menu.closest('.app-main').querySelector('.app-model').setAttribute('aria-expanded', 'false');
  });
});
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape') return;
  document.querySelectorAll('.model-menu:not([hidden])').forEach(menu => {
    menu.hidden = true;
    const trigger = menu.closest('.app-main').querySelector('.app-model');
    trigger.setAttribute('aria-expanded', 'false');
    trigger.focus({preventScroll: true});
  });
  const detail = document.querySelector('.board-task-detail');
  if (!detail.hidden) closeTask();
  if (!document.querySelector('.quick-reader').hidden) closeQuickReader();
});
document.querySelectorAll('[data-open-review]').forEach(button => button.addEventListener('click', () => {
  manualUntil = Infinity;
  setScene('review');
  if (!button.closest('#story-demo')) document.querySelector('.story-step[data-scene=review]').scrollIntoView({behavior: reducedMotion.matches ? 'instant' : 'smooth', block: 'center'});
  document.querySelector('[data-show-scene=review]').focus({preventScroll: true});
}));
let lastTaskButton;
document.querySelectorAll('[data-task]').forEach(button => button.addEventListener('click', () => {
  lastTaskButton = button;
  const panel = document.querySelector('.board-task-detail');
  panel.querySelector('h4').textContent = button.dataset.task;
  panel.hidden = false;
  panel.querySelector('button').focus({preventScroll: true});
}));
function closeTask() { document.querySelector('.board-task-detail').hidden = true; lastTaskButton?.focus({preventScroll: true}); }
document.querySelector('.close-panel').addEventListener('click', closeTask);
const workerCopy = {
  interface: ['The interface · Codex', 'The main layout is in place. I’m checking spacing and keyboard navigation before handing it back to the architect.'],
  details: ['The details · Kimi Code', 'I’m refining the smaller touches: readable colors, quiet hover states, and transitions that stay out of your way.'],
  checks: ['The checks · OpenCode', 'The interaction checks are passing. Next I’m reviewing the smaller screen layouts and reduced-motion behavior.'],
};
document.querySelectorAll('[data-worker]').forEach(button => button.addEventListener('click', () => {
  const note = document.querySelector('.team-note');
  const [title, copy] = workerCopy[button.dataset.worker];
  note.replaceChildren(Object.assign(document.createElement('strong'), {textContent: title}), document.createElement('br'), document.createTextNode(copy));
}));
document.querySelectorAll('[data-diff]').forEach(button => button.addEventListener('click', () => {
  document.querySelectorAll('[data-diff]').forEach(el => el.setAttribute('aria-pressed', String(el === button)));
  document.querySelector('.diff-content').classList.toggle('is-split', button.dataset.diff === 'split');
}));
let lastQuickTask;
document.querySelectorAll('.quick-task').forEach(button => button.addEventListener('click', () => {
  lastQuickTask = button;
  const preview = document.querySelector('#quick-preview');
  preview.querySelector('.quick-reader h3').textContent = button.querySelector('strong').textContent;
  const isCodex = button.querySelector('img').dataset.provider === 'codex';
  const response = preview.querySelector('.quick-response');
  response.querySelector('img').replaceWith(button.querySelector('img').cloneNode());
  response.querySelector('strong').firstChild.textContent = isCodex ? 'Codex ' : 'Claude Code ';
  response.querySelector('p').textContent = isCodex ? 'The interactions are working smoothly. I’m checking keyboard navigation and making sure the reduced-motion version feels just as clear.' : 'The layout is taking shape. I’m adding the responsive styles and checking the smaller screens next.';
  preview.querySelector('.quick-reader').hidden = false;
  preview.querySelector('.quick-list').hidden = true;
  preview.querySelector('.quick-back').focus({preventScroll: true});
}));
function closeQuickReader() {
  document.querySelector('.quick-reader').hidden = true;
  document.querySelector('.quick-list').hidden = false;
  lastQuickTask?.focus({preventScroll: true});
}
document.querySelector('.quick-back').addEventListener('click', closeQuickReader);
const scenery = {
  morning: ['morning-bamboo', 'A little room to breathe.', 'Monday morning, made softer.'],
  afternoon: ['lake-dawn', 'Let your thoughts wander.', 'A mountain view. A clearer mind.'],
  night: ['twilight-lake', 'Good ideas stay up late.', 'The world gets quiet. The possibilities don’t.'],
};
let sceneryRequest = 0;
document.querySelectorAll('[data-wallpaper][type=button]').forEach(button => button.addEventListener('click', async () => {
  const request = ++sceneryRequest;
  const key = button.dataset.wallpaper;
  const [file, greeting, subtitle] = scenery[key];
  const src = `/assets/${file}-${innerWidth <= 800 ? 800 : 1600}.webp`;
  const image = new Image();
  image.src = src;
  try { await image.decode(); } catch { return; }
  if (request !== sceneryRequest) return;
  const target = document.querySelector('#scenery-image');
  target.closest('picture').querySelector('source').srcset = `/assets/${file}-800.webp 800w, /assets/${file}-1600.webp 1600w`;
  target.src = src;
  document.querySelector('#scenery-preview').dataset.wallpaper = key;
  document.querySelector('#scenery-greeting').textContent = greeting;
  document.querySelector('#scenery-subtitle').textContent = subtitle;
  document.querySelectorAll('[data-wallpaper][type=button]').forEach(el => el.setAttribute('aria-pressed', String(el === button)));
  if (!reducedMotion.matches) target.animate([{opacity: .55}, {opacity: 1}], {duration: 320, easing: 'ease-out'});
}));
