import './wallpaper-settings.css';
import { escape, icon } from './dom';
import { fileAssetUrl, preview as isPreviewMode } from './bridge';
import { wallpaperAt, wallpaperPeriods, wallpaperSets, resolveWallpaperUrl, type CustomWallpaper, type WallpaperController, type WallpaperPreferences } from './wallpapers';
import { extractDominantAccent } from './wallpaper-accent-extractor';
import { palette } from './palette';

const thumbnail = (id: string): string => `/wallpapers/thumbnails/${id}.webp`;

/** Controls stay mounted during live adjustments, including native slider drags. */
export function mountWallpaperSettings(host: HTMLElement, controller: WallpaperController): () => void {
  host.classList.add('wallpaper-settings');
  host.innerHTML = `
    <figure class="wallpaper-settings-preview" aria-label="Wallpaper preview">
      <img class="wallpaper-preview-image" alt="" decoding="async">
      <div class="wallpaper-preview-scrim"></div>
      <div class="wallpaper-preview-workspace" aria-hidden="true">
        <div class="wallpaper-preview-sidebar"><i></i><i></i><i></i></div>
        <div class="wallpaper-preview-thread"><span>A little room to think.</span><div><i></i><i></i></div><div class="wallpaper-preview-composer">Describe a task…<b>↑</b></div></div>
      </div>
      <figcaption><span class="wallpaper-preview-caption"></span><span class="wallpaper-preview-badge">Live preview</span></figcaption>
    </figure>
    <div class="wallpaper-adjustments">
    <div class="wallpaper-visibility-control">
      <div class="wallpaper-control-heading"><label for="wallpaper-visibility">Wallpaper visibility</label><output class="wallpaper-visibility-output" for="wallpaper-visibility"></output></div>
      <input id="wallpaper-visibility" class="wallpaper-range-slider wallpaper-visibility-slider" type="range" min="0" max="100" step="1" aria-describedby="wallpaper-visibility-endpoints">
      <div id="wallpaper-visibility-endpoints" class="wallpaper-range-endpoints"><span>Solid</span><span>Wallpaper</span></div>
    </div>
    <div class="wallpaper-blur-control">
      <div class="wallpaper-control-heading"><label for="wallpaper-blur">Wallpaper blur</label><output class="wallpaper-blur-output" for="wallpaper-blur"></output></div>
      <input id="wallpaper-blur" class="wallpaper-range-slider" type="range" min="0" max="24" step="1" aria-describedby="wallpaper-blur-endpoints">
      <div id="wallpaper-blur-endpoints" class="wallpaper-range-endpoints"><span>Sharp</span><span>Soft</span></div>
    </div>
    <p class="wallpaper-accessibility-note" hidden>Your accessibility settings keep the workspace solid. Your wallpaper choices are saved.</p>
    </div>
    <section aria-labelledby="wallpaper-set-heading">
      <div class="wallpaper-section-heading"><h3 id="wallpaper-set-heading">Wallpaper set</h3><span>Three scenes per day</span></div>
      <div class="wallpaper-set-grid" role="group" aria-label="Wallpaper sets">
        <button type="button" class="wallpaper-set-card wallpaper-set-automatic" data-wallpaper-set="automatic" aria-label="Automatic weekly rotation" aria-pressed="false"><span class="wallpaper-set-art">${icon('refresh')}<small>7 days · 21 scenes</small></span><span class="wallpaper-set-label">Automatic<span class="wallpaper-selection-check">${icon('check')}</span></span></button>
        ${wallpaperSets.map(set => `<button type="button" class="wallpaper-set-card" data-wallpaper-set="${escape(set.id)}" aria-label="${escape(set.label)} wallpaper set" aria-pressed="false"><span class="wallpaper-set-art"><img src="${thumbnail(`${set.id}-morning`)}" alt="" loading="lazy" decoding="async"></span><span class="wallpaper-set-label">${escape(set.label)}<span class="wallpaper-selection-check">${icon('check')}</span></span></button>`).join('')}
      </div>
    </section>
    <section aria-labelledby="wallpaper-default-heading">
      <div class="wallpaper-section-heading"><h3 id="wallpaper-default-heading">Keep a favorite</h3><button type="button" class="wallpaper-resume" hidden>Resume rotation</button></div>
      <p class="wallpaper-default-help">Choose a scene to use it every day.</p>
      <div class="wallpaper-period-grid" role="group" aria-label="Default wallpaper">
        ${wallpaperPeriods.map(period => `<button type="button" class="wallpaper-period-card" data-wallpaper-period="${escape(period.id)}" aria-pressed="false"><span class="wallpaper-period-art"><img alt="" loading="lazy" decoding="async"><span class="wallpaper-default-badge">${icon('check')} Default</span></span><span class="wallpaper-period-label">${escape(period.label)}<small>${period.id === 'morning' ? '6 am – noon' : period.id === 'afternoon' ? 'Noon – 6 pm' : '6 pm – 6 am'}</small></span></button>`).join('')}
      </div>
      <p class="wallpaper-schedule-description" role="status"></p>
    </section>
    <section aria-labelledby="wallpaper-custom-heading">
      <div class="wallpaper-section-heading">
        <h3 id="wallpaper-custom-heading">Custom wallpapers</h3>
        <button type="button" class="secondary-button" data-action="add-custom-wallpaper" style="display:inline-flex;align-items:center;gap:6px;font-size:0.75rem;padding:4px 10px;border-radius:4px;">
          ${icon('plus')} Add Custom Wallpaper (Image)
        </button>
      </div>
      <p class="wallpaper-default-help">Add your own images (.png, .jpg, .webp, .svg).</p>
      <div class="wallpaper-custom-grid" role="group" aria-label="Custom wallpapers"></div>
    </section>`;

  const find = <T extends HTMLElement>(selector: string): T => host.querySelector<T>(selector)!;
  const preview = find<HTMLElement>('.wallpaper-settings-preview');
  const previewImage = find<HTMLImageElement>('.wallpaper-preview-image');
  const caption = find<HTMLElement>('.wallpaper-preview-caption');
  const slider = find<HTMLInputElement>('.wallpaper-visibility-slider');
  const output = find<HTMLOutputElement>('.wallpaper-visibility-output');
  const blurSlider = find<HTMLInputElement>('#wallpaper-blur');
  const blurOutput = find<HTMLOutputElement>('.wallpaper-blur-output');
  const accessibility = find<HTMLElement>('.wallpaper-accessibility-note');
  const resume = find<HTMLButtonElement>('.wallpaper-resume');
  const schedule = find<HTMLElement>('.wallpaper-schedule-description');
  const customGrid = find<HTMLElement>('.wallpaper-custom-grid');
  const setButtons = Array.from(host.querySelectorAll<HTMLButtonElement>('[data-wallpaper-set]'));
  const periodButtons = Array.from(host.querySelectorAll<HTMLButtonElement>('[data-wallpaper-period]'));
  let disposed = false;
  let lastCustomSignature = '';

  function displaySet(preferences: WallpaperPreferences): string {
    if (preferences.pinned) return preferences.pinned.split('-')[0];
    return preferences.set === 'automatic' ? wallpaperAt().split('-')[0] : preferences.set;
  }

  function renderCustomGrid(preferences: WallpaperPreferences): void {
    if (!customGrid) return;
    const list = preferences.custom || [];
    const signature = `${list.map(c => `${c.id}:${c.name}:${c.url}`).join('|')}#${preferences.pinned}`;
    if (signature === lastCustomSignature) return;
    lastCustomSignature = signature;

    if (!list.length) {
      customGrid.innerHTML = '<p class="wallpaper-default-help" style="margin:8px 0;opacity:0.7;">No custom wallpapers added yet.</p>';
      return;
    }
    customGrid.innerHTML = list.map(c => {
      const resolved = resolveWallpaperUrl(c.path || c.url);
      return `
      <div class="wallpaper-custom-card ${preferences.pinned === c.id ? 'selected' : ''}" data-custom-id="${escape(c.id)}">
        <div class="wallpaper-custom-media" data-action="select-custom" data-custom-id="${escape(c.id)}">
          <img src="${escape(resolved)}" alt="${escape(c.name)}"><span class="wallpaper-media-badge">IMAGE</span>
          <span class="wallpaper-custom-check">${icon('check')}</span>
        </div>
        <div class="wallpaper-custom-info">
          <span class="wallpaper-custom-name" title="${escape(c.name)}">${escape(c.name)}</span>
          <button type="button" class="icon-button wallpaper-custom-delete" data-action="delete-custom" data-custom-id="${escape(c.id)}" title="Delete custom wallpaper" aria-label="Delete ${escape(c.name)}">${icon('close')}</button>
        </div>
      </div>
    `;
    }).join('');
  }

  function render(): void {
    if (disposed) return;
    const preferences = controller.preferences;
    const id = preferences.pinned || wallpaperAt(new Date(), preferences);
    const visible = preferences.enabled && !controller.accessibilityOverride;
    const visibility = visible ? preferences.visibility : 0;
    const sliderVisibility = preferences.enabled ? preferences.visibility : 0;

    if (id.startsWith('custom-')) {
      const customItem = preferences.custom?.find(c => c.id === id);
      if (customItem) {
        const resolved = resolveWallpaperUrl(customItem.path || customItem.url);
        previewImage.style.display = 'block';
        if (preview.dataset.wallpaper !== id) {
          preview.dataset.wallpaper = id;
          previewImage.src = resolved;
          previewImage.alt = customItem.name;
        }
      }
      caption.textContent = visibility === 0 ? 'Solid background' : `Custom · ${customItem?.name ?? 'Wallpaper'}`;
    } else {
      previewImage.style.display = 'block';
      const [day, period] = id.split('-');
      const dayName = wallpaperSets.find(set => set.id === day)?.label ?? day;
      if (preview.dataset.wallpaper !== id) {
        preview.dataset.wallpaper = id;
        previewImage.src = thumbnail(id);
        previewImage.alt = `${dayName} ${period} wallpaper`;
      }
      caption.textContent = visibility === 0 ? 'Solid background' : `${dayName} · ${wallpaperPeriods.find(item => item.id === period)?.label ?? period}`;
    }

    preview.style.setProperty('--wallpaper-visibility', String(visibility / 100));
    preview.style.setProperty('--wallpaper-scrim-edge', String(1 - visibility / 100));
    preview.style.setProperty('--wallpaper-scrim-center', String((1 - visibility / 100) * (1 + .35 * visibility / 100)));
    const previewBlur = Math.round(preferences.blur * .35 * 100) / 100;
    preview.style.setProperty('--wallpaper-filter', previewBlur ? `blur(${previewBlur}px)` : 'none');
    preview.style.setProperty('--wallpaper-bleed', `${previewBlur * 3}px`);
    preview.dataset.solid = String(visibility === 0);
    if (slider.value !== String(sliderVisibility)) slider.value = String(sliderVisibility);
    slider.style.setProperty('--wallpaper-range-value', `${sliderVisibility}%`);
    slider.setAttribute('aria-valuetext', sliderVisibility === 0 ? 'Solid background, 0 percent wallpaper' : sliderVisibility === 100 ? 'Full wallpaper, 100 percent' : `${sliderVisibility} percent wallpaper`);
    output.value = `${sliderVisibility}%`;
    if (blurSlider.value !== String(preferences.blur)) blurSlider.value = String(preferences.blur);
    blurSlider.style.setProperty('--wallpaper-range-value', `${preferences.blur / 24 * 100}%`);
    blurSlider.setAttribute('aria-valuetext', preferences.blur ? `${preferences.blur} pixels of blur` : 'Off, sharp wallpaper');
    blurOutput.value = preferences.blur ? `${preferences.blur} px` : 'Off';
    accessibility.hidden = !controller.accessibilityOverride;
    for (const button of setButtons) button.setAttribute('aria-pressed', String(button.dataset.wallpaperSet === preferences.set));
    const shownSet = displaySet(preferences);
    const shownName = wallpaperSets.find(set => set.id === shownSet)?.label ?? shownSet;
    for (const button of periodButtons) {
      const time = button.dataset.wallpaperPeriod!;
      const scene = `${shownSet}-${time}`;
      button.setAttribute('aria-label', `Set ${shownName} ${time} as default`);
      button.setAttribute('aria-pressed', String(preferences.pinned === scene));
      const image = button.querySelector<HTMLImageElement>('img')!;
      if (button.dataset.wallpaper !== scene) {
        button.dataset.wallpaper = scene;
        image.src = thumbnail(scene);
      }
    }
    resume.hidden = !preferences.pinned;
    const pinnedPeriod = preferences.pinned?.split('-')[1];
    const description = preferences.pinned ? (preferences.pinned.startsWith('custom-') ? 'A custom wallpaper is your default.' : `${shownName} ${pinnedPeriod} is your default. It stays until you change it.`)
      : preferences.set === 'automatic' ? 'Follows the day of the week and your local time.'
      : `${shownName} scenes follow your local time, every day.`;
    if (schedule.textContent !== description) schedule.textContent = description;
    renderCustomGrid(preferences);
  }

  function input(): void { controller.update({ visibility: Number(slider.value) }, false); }
  function blurInput(): void { controller.update({ blur: Number(blurSlider.value) }, false); }
  function commit(): void { controller.update({}, true); }
  function click(event: MouseEvent): void {
    const el = event.target as HTMLElement;
    const target = el.closest<HTMLButtonElement>('button');
    const selectCustom = el.closest<HTMLElement>('[data-action="select-custom"]');
    if (selectCustom?.dataset.customId) {
      controller.update({ enabled: true, visibility: controller.preferences.visibility || 20, pinned: selectCustom.dataset.customId });
      return;
    }
    const deleteCustom = el.closest<HTMLElement>('[data-action="delete-custom"]');
    if (deleteCustom?.dataset.customId) {
      const id = deleteCustom.dataset.customId;
      const filtered = (controller.preferences.custom || []).filter(c => c.id !== id);
      controller.update({
        pinned: controller.preferences.pinned === id ? null : controller.preferences.pinned,
        custom: filtered
      });
      return;
    }
    const addCustom = el.closest<HTMLElement>('[data-action="add-custom-wallpaper"]');
    if (addCustom) {
      if (!isPreviewMode) {
        void (async () => {
          try {
            const dialog = await import('@tauri-apps/plugin-dialog');
            const result = await dialog.open({
              multiple: false,
              title: 'Choose custom wallpaper (Image)',
              filters: [
                { name: 'Image files', extensions: ['png', 'jpg', 'jpeg', 'webp', 'svg'] }
              ]
            });
            const path = typeof result === 'string' ? result : null;
            if (!path) return;
            const name = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'wallpaper';
            const url = fileAssetUrl(path);
            const customId = `custom-${Date.now()}`;
            const newWallpaper: CustomWallpaper = {
              id: customId,
              name,
              type: 'image',
              url,
              path,
              dateAdded: Date.now()
            };
            void extractDominantAccent(url, 'image').then(hex => {
              palette.setCustomWallpaperAccent(customId, hex);
            });
            const existing = controller.preferences.custom || [];
            controller.update({
              enabled: true,
              visibility: controller.preferences.visibility || 20,
              pinned: customId,
              custom: [...existing, newWallpaper]
            });
          } catch (e) {
            console.warn('Native dialog failed, falling back:', e);
          }
        })();
        return;
      }

      const fileInput = document.createElement('input');
      fileInput.type = 'file';
      fileInput.accept = 'image/png, image/jpeg, image/webp, image/svg+xml';
      fileInput.onchange = () => {
        const file = fileInput.files?.[0];
        if (!file) return;
        const url = URL.createObjectURL(file);
        const customId = `custom-${Date.now()}`;
        void extractDominantAccent(url, 'image').then(hex => {
          palette.setCustomWallpaperAccent(customId, hex);
        });
        const newWallpaper: CustomWallpaper = {
          id: customId,
          name: file.name,
          type: 'image',
          url,
          dateAdded: Date.now()
        };
        const existing = controller.preferences.custom || [];
        controller.update({
          enabled: true,
          visibility: controller.preferences.visibility || 20,
          pinned: customId,
          custom: [...existing, newWallpaper]
        });
      };
      fileInput.click();
      return;
    }
    if (!target || !host.contains(target)) return;
    const set = target.dataset.wallpaperSet;
    const visible = { enabled: true, visibility: controller.preferences.visibility || 20 };
    if (set) controller.update({ ...visible, set: set as WallpaperPreferences['set'], pinned: null });
    else if (target.dataset.wallpaperPeriod) controller.update({ ...visible, pinned: target.dataset.wallpaper! });
    else if (target === resume) controller.update({ ...visible, pinned: null });
  }

  slider.addEventListener('input', input);
  slider.addEventListener('change', commit);
  blurSlider.addEventListener('input', blurInput);
  blurSlider.addEventListener('change', commit);
  host.addEventListener('click', click);
  const unsubscribe = controller.subscribe(render);
  render();
  return () => {
    if (disposed) return;
    disposed = true;
    unsubscribe();
    slider.removeEventListener('input', input);
    slider.removeEventListener('change', commit);
    blurSlider.removeEventListener('input', blurInput);
    blurSlider.removeEventListener('change', commit);
    host.removeEventListener('click', click);
    controller.update({}, true);
    host.querySelectorAll('img').forEach(image => image.removeAttribute('src'));
    host.replaceChildren();
  };
}
