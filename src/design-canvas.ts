import { call } from './bridge.ts';
import { icon, toast } from './dom.ts';

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export interface DesignArtboard {
  name: string;
  filename: string;
  path: string;
  updatedAt: number;
  sizeBytes: number;
}

export type DevicePreset = 'desktop' | 'macbook' | 'ipad' | 'iphone' | 'freeform';

export interface DeviceConfig {
  id: DevicePreset;
  label: string;
  width: number;
  height: number;
  bezel: 'desktop' | 'macbook' | 'tablet' | 'phone' | 'none';
}

export const DEVICE_PRESETS: Record<DevicePreset, DeviceConfig> = {
  desktop: { id: 'desktop', label: 'Desktop 1440×900', width: 1440, height: 900, bezel: 'desktop' },
  macbook: { id: 'macbook', label: 'MacBook 1280×800', width: 1280, height: 800, bezel: 'macbook' },
  ipad: { id: 'ipad', label: 'iPad Pro 834×1194', width: 834, height: 1194, bezel: 'tablet' },
  iphone: { id: 'iphone', label: 'iPhone 16 Pro 393×852', width: 393, height: 852, bezel: 'phone' },
  freeform: { id: 'freeform', label: 'Freeform', width: 800, height: 600, bezel: 'none' },
};

export interface DesignCommentPin {
  id: string;
  artboardFilename: string;
  xPercent: number;
  yPercent: number;
  xPx?: number;
  yPx?: number;
  elementSelector: string;
  elementTag: string;
  comment: string;
}

export interface DomTreeNode {
  tag: string;
  id: string;
  classes: string[];
  selector: string;
  textSnippet: string;
  children: DomTreeNode[];
}

export type ActiveTool = 'select' | 'pan' | 'comment' | 'sketch';

export class DesignCanvas {
  private container: HTMLElement | null = null;
  private sessionId = '';
  private artboards: DesignArtboard[] = [];
  private currentArtboard: DesignArtboard | null = null;
  private pins: DesignCommentPin[] = [];
  
  // Viewport transformation
  private panX = 40;
  private panY = 40;
  private zoom = 0.75;
  private isPanning = false;
  private startMouseX = 0;
  private startMouseY = 0;
  private activeTool: ActiveTool = 'select';
  
  // Device & Artboard dimensions
  private activeDevice: DevicePreset = 'desktop';
  private customWidth = 1440;
  private customHeight = 900;
  private isResizing = false;
  private resizeStartX = 0;
  private resizeStartY = 0;
  private resizeStartW = 0;
  private resizeStartH = 0;

  // Selected element for Inspector
  private selectedElement: HTMLElement | null = null;
  private selectedSelector = '';
  private isAltDown = false;
  private hoveredElement: HTMLElement | null = null;

  // Left dock tab
  private leftDockTab: 'artboards' | 'layers' = 'artboards';
  private domTree: DomTreeNode[] = [];

  // Theme & Fullscreen
  private artboardTheme: 'light' | 'dark' = 'light';
  private isFullscreen = false;

  // Debounced auto-saver
  private saveTimeout: number | null = null;
  private onSendFeedback?: (feedbackText: string) => void;
  private eventsAbort: AbortController | null = null;

  mount(container: HTMLElement, sessionId: string, onSendFeedback?: (feedbackText: string) => void): void {
    this.eventsAbort?.abort();
    this.container = container;
    this.sessionId = sessionId;
    this.onSendFeedback = onSendFeedback;
    this.pins = [];
    this.panX = 40;
    this.panY = 40;
    this.zoom = 0.75;
    this.render();
    void this.loadArtboards();
  }

  unmount(): void {
    this.exitFullscreen();
    this.eventsAbort?.abort();
    this.eventsAbort = null;
    const modal = document.querySelector('.design-new-artboard-modal');
    if (modal) modal.remove();
  }

  setSessionId(sessionId: string): void {
    if (this.sessionId !== sessionId) {
      this.sessionId = sessionId;
      this.pins = [];
      this.selectedElement = null;
      void this.loadArtboards();
    }
  }

  async loadArtboards(): Promise<void> {
    if (!this.sessionId) return;
    try {
      this.artboards = await call<DesignArtboard[]>('design_list_artboards', { sessionId: this.sessionId });
      if (this.artboards.length > 0 && (!this.currentArtboard || !this.artboards.some(a => a.filename === this.currentArtboard!.filename))) {
        this.currentArtboard = this.artboards[0];
      } else if (this.artboards.length === 0) {
        this.currentArtboard = null;
      }
      this.render();
      if (this.currentArtboard) {
        await this.loadCurrentArtboardContent();
      }
    } catch (err) {
      console.warn('Failed to load artboards:', err);
    }
  }

  private async loadCurrentArtboardContent(): Promise<void> {
    if (!this.currentArtboard || !this.container) return;
    try {
      const html = await call<string>('design_read_artboard', {
        sessionId: this.sessionId,
        filename: this.currentArtboard.filename,
      });
      const iframe = this.container.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
      if (iframe) {
        iframe.srcdoc = html;
        iframe.onload = () => {
          this.attachIframeListeners(iframe);
          this.buildDomTree(iframe);
          this.renderLayersTree();
        };
      }
    } catch (err) {
      toast(`Failed to read artboard: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  private attachIframeListeners(iframe: HTMLIFrameElement): void {
    const doc = iframe.contentDocument;
    if (!doc) return;

    // Apply dark/light attribute to preview
    doc.documentElement.setAttribute('data-theme', this.artboardTheme);

    doc.addEventListener('keydown', e => {
      if (e.key === 'Alt') {
        this.isAltDown = true;
      }
    });
    doc.addEventListener('keyup', e => {
      if (e.key === 'Alt') {
        this.isAltDown = false;
        this.clearDistanceGuides();
      }
    });

    let originalOutline = '';
    doc.addEventListener('mouseover', e => {
      const target = (e.target as Element).closest<HTMLElement>('*');
      if (!target || target === doc.body || target === doc.documentElement) return;
      if (this.hoveredElement && this.hoveredElement !== target) {
        this.hoveredElement.style.outline = originalOutline;
      }
      this.hoveredElement = target;
      originalOutline = target.style.outline;

      if (this.activeTool === 'select' || this.activeTool === 'comment') {
        target.style.outline = '2px dashed var(--accent, #ffd700)';
        target.style.cursor = this.activeTool === 'comment' ? 'crosshair' : 'pointer';
      }

      if (this.isAltDown && this.selectedElement && target !== this.selectedElement) {
        this.drawDistanceGuides(this.selectedElement, target);
      }
    });

    doc.addEventListener('mouseout', e => {
      const target = (e.target as Element).closest<HTMLElement>('*');
      if (target && target === this.hoveredElement) {
        target.style.outline = originalOutline;
        this.hoveredElement = null;
        this.clearDistanceGuides();
      }
    });

    doc.defaultView?.addEventListener('scroll', () => {
      this.renderPins();
    });

    doc.addEventListener('click', e => {
      e.preventDefault();
      e.stopPropagation();
      const target = (e.target as Element).closest<HTMLElement>('*');
      if (!target || !this.currentArtboard) return;

      const scrollX = doc.defaultView?.scrollX || 0;
      const scrollY = doc.defaultView?.scrollY || 0;
      const docW = doc.documentElement.scrollWidth || iframe.clientWidth || 1;
      const docH = doc.documentElement.scrollHeight || iframe.clientHeight || 1;
      const xPx = e.clientX + scrollX;
      const yPx = e.clientY + scrollY;
      const xPercent = Math.max(0, Math.min(100, (xPx / docW) * 100));
      const yPercent = Math.max(0, Math.min(100, (yPx / docH) * 100));

      const tag = target.tagName.toLowerCase();
      const idStr = target.id ? `#${target.id}` : '';
      const clsStr = target.className && typeof target.className === 'string'
        ? `.${target.className.trim().split(/\s+/).slice(0, 2).join('.')}`
        : '';
      const selector = `${tag}${idStr}${clsStr}`;

      if (this.activeTool === 'comment') {
        this.showAddCommentPinModal(selector, tag, xPercent, yPercent, xPx, yPx);
      } else {
        // Select tool
        this.selectElement(target, selector);
      }
    });
  }

  private selectElement(element: HTMLElement, selector: string): void {
    if (this.selectedElement) {
      this.selectedElement.style.outline = '';
    }
    this.selectedElement = element;
    this.selectedSelector = selector;
    element.style.outline = '2px solid var(--accent, #ffd700)';
    this.renderPropertyInspector();
    this.highlightLayerInTree(selector);
  }

  private buildDomTree(iframe: HTMLIFrameElement): void {
    const doc = iframe.contentDocument;
    if (!doc || !doc.body) {
      this.domTree = [];
      return;
    }

    const traverse = (el: Element): DomTreeNode | null => {
      const tag = el.tagName.toLowerCase();
      if (['script', 'style', 'meta', 'link'].includes(tag)) return null;

      const id = el.id || '';
      const classes = el.className && typeof el.className === 'string'
        ? el.className.trim().split(/\s+/).filter(Boolean)
        : [];
      const selector = `${tag}${id ? '#' + id : ''}${classes.length ? '.' + classes.slice(0, 2).join('.') : ''}`;
      
      let textSnippet = '';
      if (el.childNodes.length === 1 && el.childNodes[0].nodeType === Node.TEXT_NODE) {
        textSnippet = (el.textContent || '').trim().slice(0, 20);
      }

      const children: DomTreeNode[] = [];
      for (const child of Array.from(el.children)) {
        const node = traverse(child);
        if (node) children.push(node);
      }

      return { tag, id, classes, selector, textSnippet, children };
    };

    const rootNode = traverse(doc.body);
    this.domTree = rootNode ? rootNode.children : [];
  }

  private render(): void {
    if (!this.container) return;

    if (this.artboards.length === 0) {
      this.container.innerHTML = `
        <div class="design-studio-root empty">
          <div class="design-empty-box">
            <span class="design-empty-icon">${icon('spark')}</span>
            <h2>Figma-Class Design Canvas</h2>
            <p>Direct visual manipulation, instant HTML rendering, and multi-device frames.</p>
            <div class="design-preset-buttons">
              <button type="button" class="design-preset-btn" data-create-preset="dashboard">
                ${icon('monitor')}
                <div>
                  <strong>SaaS Dashboard</strong>
                  <span>1440 × 900 px</span>
                </div>
              </button>
              <button type="button" class="design-preset-btn" data-create-preset="mobile">
                ${icon('phone')}
                <div>
                  <strong>Mobile App Screen</strong>
                  <span>393 × 852 px</span>
                </div>
              </button>
              <button type="button" class="design-preset-btn" data-create-preset="modal">
                ${icon('window')}
                <div>
                  <strong>Auth Modal / Dialog</strong>
                  <span>480 × 600 px</span>
                </div>
              </button>
              <button type="button" class="design-preset-btn" data-create-preset="blank">
                ${icon('plus')}
                <div>
                  <strong>Blank Canvas</strong>
                  <span>800 × 600 px</span>
                </div>
              </button>
            </div>
          </div>
        </div>
      `;
      this.bindEmptyStateEvents();
      return;
    }

    const deviceConfig = DEVICE_PRESETS[this.activeDevice];
    const frameW = this.activeDevice === 'freeform' ? this.customWidth : deviceConfig.width;
    const frameH = this.activeDevice === 'freeform' ? this.customHeight : deviceConfig.height;

    this.container.innerHTML = `
      <div class="design-studio-root ${this.isFullscreen ? 'is-fullscreen' : ''}">
        
        <!-- Top Studio Bar -->
        <header class="design-studio-header">
          <div class="design-header-left">
            <div class="design-artboard-picker">
              <select class="design-artboard-select" aria-label="Select artboard">
                ${this.artboards.map(a => `<option value="${escapeHtml(a.filename)}" ${this.currentArtboard?.filename === a.filename ? 'selected' : ''}>${escapeHtml(a.name)}</option>`).join('')}
              </select>
              <button type="button" class="design-icon-btn" data-action="new-artboard-menu" title="New Artboard Preset">${icon('plus')}</button>
            </div>
            <div class="design-device-switcher">
              ${Object.values(DEVICE_PRESETS).map(d => `
                <button type="button" class="design-device-btn ${this.activeDevice === d.id ? 'active' : ''}" data-device="${d.id}" title="${d.label}">
                  ${d.id === 'desktop' || d.id === 'macbook' ? icon('monitor') : d.id === 'ipad' ? icon('tablet') : d.id === 'iphone' ? icon('phone') : icon('expand')}
                  <span>${d.id.charAt(0).toUpperCase() + d.id.slice(1)}</span>
                </button>
              `).join('')}
            </div>
          </div>

          <div class="design-header-right">
            <div class="design-zoom-group">
              <button type="button" class="design-icon-btn" data-action="zoom-out" title="Zoom out">-</button>
              <span class="design-zoom-val">${Math.round(this.zoom * 100)}%</span>
              <button type="button" class="design-icon-btn" data-action="zoom-in" title="Zoom in">+</button>
              <button type="button" class="design-icon-btn" data-action="zoom-fit" title="Zoom to fit">Fit</button>
            </div>
            <button type="button" class="design-icon-btn ${this.artboardTheme === 'dark' ? 'active' : ''}" data-action="toggle-theme" title="Toggle Light/Dark Theme">
              ${this.artboardTheme === 'dark' ? icon('moon') : icon('sun')}
            </button>
            <button type="button" class="design-icon-btn ${this.isFullscreen ? 'active' : ''}" data-action="toggle-fullscreen" title="Toggle Studio Fullscreen">
              ${this.isFullscreen ? icon('minimize') : icon('expand')}
            </button>
            <span class="design-save-indicator" id="design-save-indicator" title="Auto-saved to disk">
              ${icon('check')} <span>Saved</span>
            </span>
          </div>
        </header>

        <!-- 3-Pane Body -->
        <div class="design-studio-body">
          
          <!-- LEFT DOCK: Artboards & Layers -->
          <aside class="design-left-dock">
            <div class="design-dock-tabs">
              <button type="button" class="design-dock-tab ${this.leftDockTab === 'artboards' ? 'active' : ''}" data-tab="artboards">Artboards</button>
              <button type="button" class="design-dock-tab ${this.leftDockTab === 'layers' ? 'active' : ''}" data-tab="layers">DOM Layers</button>
            </div>
            
            <div class="design-dock-content">
              <!-- Artboards List -->
              <div class="design-tab-pane ${this.leftDockTab === 'artboards' ? 'active' : ''}" id="design-pane-artboards">
                <div class="design-artboards-list">
                  ${this.artboards.map(a => `
                    <div class="design-artboard-item ${this.currentArtboard?.filename === a.filename ? 'selected' : ''}" data-filename="${escapeHtml(a.filename)}">
                      <div class="design-item-info">
                        <span class="design-item-name">${escapeHtml(a.name)}</span>
                        <span class="design-item-file">${escapeHtml(a.filename)}</span>
                      </div>
                      <div class="design-item-actions">
                        <button type="button" class="design-item-btn" data-action="duplicate-artboard" data-filename="${escapeHtml(a.filename)}" title="Duplicate">${icon('copy')}</button>
                        <button type="button" class="design-item-btn" data-action="delete-artboard" data-filename="${escapeHtml(a.filename)}" title="Delete">${icon('trash')}</button>
                      </div>
                    </div>
                  `).join('')}
                </div>
              </div>

              <!-- DOM Layers Tree -->
              <div class="design-tab-pane ${this.leftDockTab === 'layers' ? 'active' : ''}" id="design-pane-layers">
                <div class="design-layers-tree" id="design-layers-tree">
                  <!-- Rendered dynamically -->
                </div>
              </div>
            </div>
          </aside>

          <!-- CENTER VIEWPORT: Canvas with Rulers & Device Frame -->
          <main class="design-viewport" role="region" aria-label="Infinite Design Canvas">
            <div class="design-ruler ruler-top"></div>
            <div class="design-ruler ruler-left"></div>

            <div class="design-infinite-stage" style="transform: translate(${this.panX}px, ${this.panY}px) scale(${this.zoom});">
              
              <!-- Frame Bezel -->
              <div class="design-device-frame bezel-${deviceConfig.bezel}" style="width: ${frameW}px; height: ${frameH}px;">
                
                ${deviceConfig.bezel === 'desktop' ? `
                  <div class="design-bezel-titlebar">
                    <span class="design-bezel-dots"><span class="dot-red"></span><span class="dot-yellow"></span><span class="dot-green"></span></span>
                    <span class="design-bezel-title">${escapeHtml(this.currentArtboard?.name || 'Artboard')}</span>
                  </div>
                ` : ''}

                ${deviceConfig.bezel === 'phone' ? `
                  <div class="design-dynamic-island">
                    <div class="dynamic-island-pill"></div>
                  </div>
                ` : ''}

                <div class="design-frame-content">
                  <iframe class="design-artboard-iframe" sandbox="allow-same-origin allow-scripts"></iframe>
                  <div class="design-pin-layer"></div>
                  <div class="design-guides-layer"></div>
                </div>

                <!-- Resize handle for freeform -->
                <div class="design-resize-handle" title="Drag to resize artboard"></div>
              </div>
            </div>

            <!-- Floating Bottom Tools -->
            <div class="design-bottom-toolbar">
              <button type="button" class="design-tool-btn ${this.activeTool === 'select' ? 'active' : ''}" data-tool="select" title="Select / Direct Edit (V)">
                ${icon('cursor')}<span>Select</span>
              </button>
              <button type="button" class="design-tool-btn ${this.activeTool === 'pan' ? 'active' : ''}" data-tool="pan" title="Pan Hand Tool (H)">
                ${icon('hand')}<span>Pan</span>
              </button>
              <button type="button" class="design-tool-btn ${this.activeTool === 'comment' ? 'active' : ''}" data-tool="comment" title="Add Visual Comment (C)">
                ${icon('pin')}<span>Comment</span>
              </button>
              <span class="design-toolbar-divider"></span>
              <button type="button" class="design-tool-btn" data-action="ai-remix" title="Ask AI to remix or restyle UI">
                ${icon('spark')}<span>AI Remix</span>
              </button>
              <button type="button" class="design-tool-btn" data-action="convert-code" title="Convert Artboard to Production Code">
                ${icon('zap')}<span>Export Code</span>
              </button>
            </div>

            <!-- Minimap -->
            <div class="design-minimap" title="Minimap Overview">
              <div class="design-minimap-viewport"></div>
            </div>
          </main>

          <!-- RIGHT DOCK: Live Visual CSS Property Inspector -->
          <aside class="design-right-dock">
            <div class="design-inspector-header">
              <span class="design-inspector-title">Property Inspector</span>
              <span class="design-selected-tag">${this.selectedSelector ? escapeHtml(this.selectedSelector) : 'No element selected'}</span>
            </div>
            
            <div class="design-inspector-scroll" id="design-inspector-fields">
              <!-- Rendered dynamically -->
              <p class="design-inspector-empty">Click any element in the artboard with the Select tool (V) to edit text, flexbox, box model, colors, and typography in real time.</p>
            </div>
          </aside>

        </div>

        <!-- Pinned Feedback Drawer -->
        <div class="design-comments-drawer ${this.pins.length > 0 ? 'is-open' : ''}">
          <div class="design-comments-head">
            <span>Visual Feedback (${this.pins.length})</span>
            <button type="button" class="design-icon-btn" data-action="clear-pins" title="Clear all pins">${icon('trash')}</button>
          </div>
          <div class="design-comments-scroll"></div>
          <div class="design-comments-foot">
            <button type="button" class="design-send-agent-btn" data-action="send-agent-feedback">
              ${icon('spark')} Send ${this.pins.length} to Agent
            </button>
          </div>
        </div>

      </div>
    `;

    this.bindEvents();
    this.renderLayersTree();
    this.renderPins();
    this.renderCommentsList();
    this.renderPropertyInspector();
  }

  private bindEvents(): void {
    if (!this.container) return;
    this.eventsAbort?.abort();
    this.eventsAbort = new AbortController();
    const { signal } = this.eventsAbort;

    // Artboard select
    const select = this.container.querySelector<HTMLSelectElement>('.design-artboard-select');
    select?.addEventListener('change', async () => {
      const chosen = this.artboards.find(a => a.filename === select.value);
      if (chosen) {
        this.currentArtboard = chosen;
        this.selectedElement = null;
        this.render();
        await this.loadCurrentArtboardContent();
      }
    }, { signal });

    // Device switcher
    this.container.querySelectorAll<HTMLButtonElement>('[data-device]').forEach(btn => {
      btn.addEventListener('click', () => {
        this.activeDevice = btn.dataset.device as DevicePreset;
        this.render();
        void this.loadCurrentArtboardContent();
      }, { signal });
    });

    // Dock tabs
    this.container.querySelectorAll<HTMLButtonElement>('.design-dock-tab').forEach(tab => {
      tab.addEventListener('click', () => {
        this.leftDockTab = tab.dataset.tab as 'artboards' | 'layers';
        this.container?.querySelectorAll('.design-dock-tab').forEach(t => t.classList.toggle('active', t === tab));
        this.container?.querySelectorAll('.design-tab-pane').forEach(p => {
          p.classList.toggle('active', p.id === `design-pane-${this.leftDockTab}`);
        });
      }, { signal });
    });

    // Tool selector
    this.container.querySelectorAll<HTMLButtonElement>('[data-tool]').forEach(btn => {
      btn.addEventListener('click', () => {
        this.activeTool = btn.dataset.tool as ActiveTool;
        this.container?.querySelectorAll('[data-tool]').forEach(b => b.classList.toggle('active', b === btn));
      }, { signal });
    });

    // Actions delegation
    this.container.addEventListener('click', async e => {
      const btn = (e.target as Element).closest<HTMLElement>('[data-action]');
      if (!btn) return;
      const action = btn.dataset.action;

      if (action === 'zoom-in') {
        this.zoom = Math.min(2.5, this.zoom + 0.15);
        this.updateStageTransform();
      } else if (action === 'zoom-out') {
        this.zoom = Math.max(0.2, this.zoom - 0.15);
        this.updateStageTransform();
      } else if (action === 'zoom-fit') {
        this.zoom = 0.75;
        this.panX = 40;
        this.panY = 40;
        this.updateStageTransform();
      } else if (action === 'toggle-theme') {
        this.artboardTheme = this.artboardTheme === 'light' ? 'dark' : 'light';
        const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
        if (iframe?.contentDocument) {
          iframe.contentDocument.documentElement.setAttribute('data-theme', this.artboardTheme);
        }
        btn.innerHTML = this.artboardTheme === 'dark' ? icon('moon') : icon('sun');
        btn.classList.toggle('active', this.artboardTheme === 'dark');
      } else if (action === 'toggle-fullscreen') {
        this.toggleFullscreen();
      } else if (action === 'new-artboard-menu') {
        this.showNewArtboardModal();
      } else if (action === 'duplicate-artboard') {
        const file = btn.dataset.filename;
        if (file) await this.duplicateArtboard(file);
      } else if (action === 'delete-artboard') {
        const file = btn.dataset.filename;
        if (file) await this.deleteArtboard(file);
      } else if (action === 'clear-pins') {
        this.pins = [];
        this.renderPins();
        this.renderCommentsList();
      } else if (action === 'send-agent-feedback') {
        this.sendAgentFeedback();
      } else if (action === 'ai-remix') {
        this.promptAiRemix();
      } else if (action === 'convert-code') {
        this.promptConvertCode();
      }
    }, { signal });

    // Canvas Pan & Zoom
    this.bindPanZoom(signal);

    // Canvas Freeform Resize
    this.bindFreeformResize(signal);

    // Escape exits studio fullscreen mode
    window.addEventListener('keydown', e => {
      if (e.key === 'Escape' && this.isFullscreen) {
        this.toggleFullscreen(false);
      }
    }, { signal });
  }

  public toggleFullscreen(force?: boolean): void {
    this.isFullscreen = force !== undefined ? force : !this.isFullscreen;
    document.documentElement.classList.toggle('design-fullscreen', this.isFullscreen);
    this.container?.querySelector('.design-studio-root')?.classList.toggle('is-fullscreen', this.isFullscreen);
    const fsBtn = this.container?.querySelector<HTMLElement>('[data-action="toggle-fullscreen"]');
    if (fsBtn) {
      fsBtn.innerHTML = this.isFullscreen ? icon('minimize') : icon('expand');
      fsBtn.classList.toggle('active', this.isFullscreen);
    }
  }

  public exitFullscreen(): void {
    if (this.isFullscreen) {
      this.toggleFullscreen(false);
    }
  }

  private bindPanZoom(signal: AbortSignal): void {
    const viewport = this.container?.querySelector<HTMLElement>('.design-viewport');
    if (!viewport) return;

    viewport.addEventListener('mousedown', e => {
      // Middle click or Hand tool or Space key held
      if (e.button === 1 || this.activeTool === 'pan' || (e.target as Element).closest('.design-infinite-stage') === null) {
        this.isPanning = true;
        this.startMouseX = e.clientX;
        this.startMouseY = e.clientY;
        viewport.style.cursor = 'grabbing';
      }
    }, { signal });

    window.addEventListener('mousemove', e => {
      if (!this.isPanning) return;
      const dx = e.clientX - this.startMouseX;
      const dy = e.clientY - this.startMouseY;
      this.startMouseX = e.clientX;
      this.startMouseY = e.clientY;
      this.panX += dx;
      this.panY += dy;
      this.updateStageTransform();
    }, { signal });

    window.addEventListener('mouseup', () => {
      if (this.isPanning) {
        this.isPanning = false;
        if (viewport) viewport.style.cursor = this.activeTool === 'pan' ? 'grab' : 'default';
      }
    }, { signal });

    viewport.addEventListener('wheel', e => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        const factor = e.deltaY < 0 ? 1.08 : 0.92;
        this.zoom = Math.max(0.15, Math.min(3.0, this.zoom * factor));
        this.updateStageTransform();
      } else {
        this.panX -= e.deltaX;
        this.panY -= e.deltaY;
        this.updateStageTransform();
      }
    }, { signal, passive: false });
  }

  private bindFreeformResize(signal: AbortSignal): void {
    const handle = this.container?.querySelector<HTMLElement>('.design-resize-handle');
    if (!handle) return;

    handle.addEventListener('mousedown', e => {
      e.stopPropagation();
      e.preventDefault();
      this.isResizing = true;
      this.resizeStartX = e.clientX;
      this.resizeStartY = e.clientY;
      this.resizeStartW = this.customWidth;
      this.resizeStartH = this.customHeight;
    }, { signal });

    window.addEventListener('mousemove', e => {
      if (!this.isResizing) return;
      const dx = (e.clientX - this.resizeStartX) / this.zoom;
      const dy = (e.clientY - this.resizeStartY) / this.zoom;
      this.customWidth = Math.max(320, Math.round(this.resizeStartW + dx));
      this.customHeight = Math.max(240, Math.round(this.resizeStartH + dy));
      const frame = this.container?.querySelector<HTMLElement>('.design-device-frame');
      if (frame) {
        frame.style.width = `${this.customWidth}px`;
        frame.style.height = `${this.customHeight}px`;
      }
    }, { signal });

    window.addEventListener('mouseup', () => {
      this.isResizing = false;
    }, { signal });
  }

  private updateStageTransform(): void {
    const stage = this.container?.querySelector<HTMLElement>('.design-infinite-stage');
    if (stage) {
      stage.style.transform = `translate(${this.panX}px, ${this.panY}px) scale(${this.zoom})`;
    }
    const val = this.container?.querySelector<HTMLElement>('.design-zoom-val');
    if (val) {
      val.textContent = `${Math.round(this.zoom * 100)}%`;
    }
  }

  private renderLayersTree(): void {
    const treeEl = this.container?.querySelector<HTMLElement>('#design-layers-tree');
    if (!treeEl) return;

    if (this.domTree.length === 0) {
      treeEl.innerHTML = '<p class="design-layers-empty">No DOM elements found.</p>';
      return;
    }

    const renderNode = (node: DomTreeNode): string => {
      const hasChildren = node.children.length > 0;
      return `
        <div class="design-layer-row" data-selector="${escapeHtml(node.selector)}">
          <span class="design-layer-caret ${hasChildren ? 'has-children' : ''}"></span>
          <span class="design-layer-tag">&lt;${escapeHtml(node.tag)}&gt;</span>
          ${node.id ? `<span class="design-layer-id">#${escapeHtml(node.id)}</span>` : ''}
          ${node.textSnippet ? `<span class="design-layer-snippet">"${escapeHtml(node.textSnippet)}"</span>` : ''}
        </div>
        ${hasChildren ? `<div class="design-layer-children">${node.children.map(renderNode).join('')}</div>` : ''}
      `;
    };

    treeEl.innerHTML = this.domTree.map(renderNode).join('');

    // Bind layer click & hover
    treeEl.querySelectorAll<HTMLElement>('.design-layer-row').forEach(row => {
      const sel = row.dataset.selector;
      if (!sel) return;

      row.addEventListener('mouseenter', () => {
        const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
        const target = iframe?.contentDocument?.querySelector<HTMLElement>(sel);
        if (target) target.style.outline = '2px dashed var(--accent, #ffd700)';
      });

      row.addEventListener('mouseleave', () => {
        const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
        const target = iframe?.contentDocument?.querySelector<HTMLElement>(sel);
        if (target && target !== this.selectedElement) target.style.outline = '';
      });

      row.addEventListener('click', () => {
        const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
        const target = iframe?.contentDocument?.querySelector<HTMLElement>(sel);
        if (target) this.selectElement(target, sel);
      });
    });
  }

  private highlightLayerInTree(selector: string): void {
    const treeEl = this.container?.querySelector<HTMLElement>('#design-layers-tree');
    if (!treeEl) return;
    treeEl.querySelectorAll('.design-layer-row').forEach(r => r.classList.remove('selected'));
    const matched = treeEl.querySelector<HTMLElement>(`.design-layer-row[data-selector="${selector}"]`);
    if (matched) {
      matched.classList.add('selected');
      matched.scrollIntoView({ block: 'nearest' });
    }
  }

  private renderPropertyInspector(): void {
    const fieldsEl = this.container?.querySelector<HTMLElement>('#design-inspector-fields');
    if (!fieldsEl) return;

    if (!this.selectedElement) {
      fieldsEl.innerHTML = '<p class="design-inspector-empty">Select any element in the artboard with the Select tool (V) to edit text, flexbox, box model, colors, and typography in real time.</p>';
      return;
    }

    const el = this.selectedElement;
    const computed = el.ownerDocument.defaultView ? el.ownerDocument.defaultView.getComputedStyle(el) : null;
    const textContent = el.childNodes.length === 1 && el.childNodes[0].nodeType === Node.TEXT_NODE
      ? el.textContent || ''
      : '';

    const display = computed?.display || 'block';
    const isFlex = display === 'flex' || display === 'inline-flex';
    const isGrid = display === 'grid' || display === 'inline-grid';

    fieldsEl.innerHTML = `
      <!-- Text / Content -->
      <div class="design-prop-group">
        <label class="design-prop-group-title">Content & Text</label>
        <textarea class="design-input-area" id="prop-text" rows="2" placeholder="Text snippet...">${escapeHtml(textContent)}</textarea>
      </div>

      <!-- Layout & Flexbox -->
      <div class="design-prop-group">
        <label class="design-prop-group-title">Layout</label>
        <div class="design-prop-row">
          <span class="design-prop-label">Display</span>
          <select class="design-select" id="prop-display">
            <option value="block" ${display === 'block' ? 'selected' : ''}>Block</option>
            <option value="flex" ${isFlex ? 'selected' : ''}>Flex</option>
            <option value="grid" ${isGrid ? 'selected' : ''}>Grid</option>
            <option value="inline-block" ${display === 'inline-block' ? 'selected' : ''}>Inline-Block</option>
            <option value="none" ${display === 'none' ? 'selected' : ''}>None</option>
          </select>
        </div>
        ${isFlex ? `
          <div class="design-prop-row">
            <span class="design-prop-label">Direction</span>
            <select class="design-select" id="prop-flex-direction">
              <option value="row" ${computed?.flexDirection === 'row' ? 'selected' : ''}>Row</option>
              <option value="column" ${computed?.flexDirection === 'column' ? 'selected' : ''}>Column</option>
            </select>
          </div>
          <div class="design-prop-row">
            <span class="design-prop-label">Align</span>
            <select class="design-select" id="prop-align-items">
              <option value="stretch" ${computed?.alignItems === 'stretch' ? 'selected' : ''}>Stretch</option>
              <option value="center" ${computed?.alignItems === 'center' ? 'selected' : ''}>Center</option>
              <option value="flex-start" ${computed?.alignItems === 'flex-start' ? 'selected' : ''}>Start</option>
              <option value="flex-end" ${computed?.alignItems === 'flex-end' ? 'selected' : ''}>End</option>
            </select>
          </div>
          <div class="design-prop-row">
            <span class="design-prop-label">Justify</span>
            <select class="design-select" id="prop-justify-content">
              <option value="flex-start" ${computed?.justifyContent === 'flex-start' ? 'selected' : ''}>Start</option>
              <option value="center" ${computed?.justifyContent === 'center' ? 'selected' : ''}>Center</option>
              <option value="space-between" ${computed?.justifyContent === 'space-between' ? 'selected' : ''}>Space Between</option>
              <option value="space-around" ${computed?.justifyContent === 'space-around' ? 'selected' : ''}>Space Around</option>
            </select>
          </div>
        ` : ''}
        ${isFlex || isGrid ? `
          <div class="design-prop-row">
            <span class="design-prop-label">Gap</span>
            <input type="text" class="design-input" id="prop-gap" value="${computed?.gap || '0px'}">
          </div>
        ` : ''}
      </div>

      <!-- Dimensions -->
      <div class="design-prop-group">
        <label class="design-prop-group-title">Dimensions</label>
        <div class="design-prop-grid2">
          <div>
            <span class="design-prop-sub">Width</span>
            <input type="text" class="design-input" id="prop-width" value="${el.style.width || computed?.width || 'auto'}">
          </div>
          <div>
            <span class="design-prop-sub">Height</span>
            <input type="text" class="design-input" id="prop-height" value="${el.style.height || computed?.height || 'auto'}">
          </div>
        </div>
      </div>

      <!-- Typography -->
      <div class="design-prop-group">
        <label class="design-prop-group-title">Typography</label>
        <div class="design-prop-grid2">
          <div>
            <span class="design-prop-sub">Font Size</span>
            <input type="text" class="design-input" id="prop-font-size" value="${computed?.fontSize || '16px'}">
          </div>
          <div>
            <span class="design-prop-sub">Weight</span>
            <select class="design-select" id="prop-font-weight">
              <option value="400" ${computed?.fontWeight === '400' ? 'selected' : ''}>400 (Regular)</option>
              <option value="500" ${computed?.fontWeight === '500' ? 'selected' : ''}>500 (Medium)</option>
              <option value="600" ${computed?.fontWeight === '600' ? 'selected' : ''}>600 (Semibold)</option>
              <option value="700" ${computed?.fontWeight === '700' ? 'selected' : ''}>700 (Bold)</option>
            </select>
          </div>
        </div>
        <div class="design-prop-row" style="margin-top: 6px;">
          <span class="design-prop-label">Color</span>
          <div class="design-color-picker-wrap">
            <input type="color" class="design-color-picker" id="prop-color-pick" value="${this.rgbToHex(computed?.color || '#000000')}">
            <input type="text" class="design-input design-color-text" id="prop-color" value="${computed?.color || ''}">
          </div>
        </div>
      </div>

      <!-- Appearance -->
      <div class="design-prop-group">
        <label class="design-prop-group-title">Appearance</label>
        <div class="design-prop-row">
          <span class="design-prop-label">Background</span>
          <div class="design-color-picker-wrap">
            <input type="color" class="design-color-picker" id="prop-bg-pick" value="${this.rgbToHex(computed?.backgroundColor || '#ffffff')}">
            <input type="text" class="design-input design-color-text" id="prop-bg" value="${computed?.backgroundColor || ''}">
          </div>
        </div>
        <div class="design-prop-row">
          <span class="design-prop-label">Corner Radius</span>
          <input type="text" class="design-input" id="prop-radius" value="${computed?.borderRadius || '0px'}">
        </div>
        <div class="design-prop-row">
          <span class="design-prop-label">Opacity</span>
          <input type="range" min="0" max="1" step="0.05" class="design-range" id="prop-opacity" value="${computed?.opacity || '1'}">
        </div>
      </div>
    `;

    // Bind real-time input handlers
    this.bindPropertyInputs(el);
  }

  private bindPropertyInputs(el: HTMLElement): void {
    const bindLive = (id: string, cssProp: string) => {
      const input = this.container?.querySelector<HTMLInputElement | HTMLSelectElement>(`#${id}`);
      input?.addEventListener('input', () => {
        el.style.setProperty(cssProp, input.value);
        this.scheduleAutoSave();
      });
    };

    // Text content
    const textInput = this.container?.querySelector<HTMLTextAreaElement>('#prop-text');
    textInput?.addEventListener('input', () => {
      el.textContent = textInput.value;
      this.scheduleAutoSave();
    });

    bindLive('prop-display', 'display');
    bindLive('prop-flex-direction', 'flex-direction');
    bindLive('prop-align-items', 'align-items');
    bindLive('prop-justify-content', 'justify-content');
    bindLive('prop-gap', 'gap');
    bindLive('prop-width', 'width');
    bindLive('prop-height', 'height');
    bindLive('prop-font-size', 'font-size');
    bindLive('prop-font-weight', 'font-weight');
    bindLive('prop-radius', 'border-radius');
    bindLive('prop-opacity', 'opacity');

    // Colors
    const colorText = this.container?.querySelector<HTMLInputElement>('#prop-color');
    const colorPick = this.container?.querySelector<HTMLInputElement>('#prop-color-pick');
    colorPick?.addEventListener('input', () => {
      if (colorText) colorText.value = colorPick.value;
      el.style.color = colorPick.value;
      this.scheduleAutoSave();
    });
    colorText?.addEventListener('input', () => {
      el.style.color = colorText.value;
      this.scheduleAutoSave();
    });

    const bgText = this.container?.querySelector<HTMLInputElement>('#prop-bg');
    const bgPick = this.container?.querySelector<HTMLInputElement>('#prop-bg-pick');
    bgPick?.addEventListener('input', () => {
      if (bgText) bgText.value = bgPick.value;
      el.style.backgroundColor = bgPick.value;
      this.scheduleAutoSave();
    });
    bgText?.addEventListener('input', () => {
      el.style.backgroundColor = bgText.value;
      this.scheduleAutoSave();
    });
  }

  private scheduleAutoSave(): void {
    const indicator = this.container?.querySelector<HTMLElement>('#design-save-indicator');
    if (indicator) {
      indicator.classList.add('saving');
      indicator.innerHTML = `${icon('refresh')} <span>Saving…</span>`;
    }

    if (this.saveTimeout !== null) {
      window.clearTimeout(this.saveTimeout);
    }

    this.saveTimeout = window.setTimeout(async () => {
      await this.saveCurrentArtboard();
      if (indicator) {
        indicator.classList.remove('saving');
        indicator.innerHTML = `${icon('check')} <span>Saved</span>`;
      }
    }, 400);
  }

  private async saveCurrentArtboard(): Promise<void> {
    if (!this.currentArtboard) return;
    const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
    const doc = iframe?.contentDocument;
    if (!doc || !doc.documentElement) return;

    // Clean any temporary outlines before saving
    const tempCleanups: { el: HTMLElement; outline: string }[] = [];
    doc.querySelectorAll<HTMLElement>('*').forEach(e => {
      if (e.style.outline) {
        tempCleanups.push({ el: e, outline: e.style.outline });
        e.style.outline = '';
      }
    });

    const htmlContent = '<!DOCTYPE html>\n' + doc.documentElement.outerHTML;

    // Restore outlines for canvas editor
    for (const item of tempCleanups) {
      item.el.style.outline = item.outline;
    }

    try {
      await call('design_save_artboard', {
        sessionId: this.sessionId,
        filename: this.currentArtboard.filename,
        htmlContent,
      });
    } catch (e) {
      console.warn('Auto-save artboard failed:', e);
    }
  }

  private drawDistanceGuides(selected: HTMLElement, hovered: HTMLElement): void {
    const layer = this.container?.querySelector<HTMLElement>('.design-guides-layer');
    if (!layer) return;

    const r1 = selected.getBoundingClientRect();
    const r2 = hovered.getBoundingClientRect();

    const dx = Math.round(Math.abs(r1.left - r2.left));
    const dy = Math.round(Math.abs(r1.top - r2.top));

    layer.innerHTML = `
      <div class="design-guide-line" style="left:${Math.min(r1.left, r2.left)}px; top:${r1.top}px; width:${dx}px; height:1px;"></div>
      <div class="design-guide-badge" style="left:${Math.min(r1.left, r2.left) + dx / 2}px; top:${r1.top - 12}px;">${dx}px</div>
    `;
  }

  private clearDistanceGuides(): void {
    const layer = this.container?.querySelector<HTMLElement>('.design-guides-layer');
    if (layer) layer.innerHTML = '';
  }

  private renderPins(): void {
    const pinLayer = this.container?.querySelector<HTMLElement>('.design-pin-layer');
    if (!pinLayer) return;
    const iframe = this.container?.querySelector<HTMLIFrameElement>('.design-artboard-iframe');
    const doc = iframe?.contentDocument;
    const scrollX = doc?.defaultView?.scrollX || 0;
    const scrollY = doc?.defaultView?.scrollY || 0;

    pinLayer.innerHTML = this.pins.map((pin, i) => {
      let left = `${pin.xPercent}%`;
      let top = `${pin.yPercent}%`;
      if (typeof pin.xPx === 'number' && typeof pin.yPx === 'number') {
        left = `${pin.xPx - scrollX}px`;
        top = `${pin.yPx - scrollY}px`;
      }
      return `
        <div class="design-pin-marker" style="left:${left}; top:${top};" title="${escapeHtml(pin.comment)}">
          <span>${i + 1}</span>
        </div>
      `;
    }).join('');
  }

  private renderCommentsList(): void {
    const drawer = this.container?.querySelector<HTMLElement>('.design-comments-drawer');
    const scroll = this.container?.querySelector<HTMLElement>('.design-comments-scroll');
    if (!drawer || !scroll) return;

    drawer.classList.toggle('is-open', this.pins.length > 0);

    scroll.innerHTML = this.pins.map((pin, i) => `
      <div class="design-comment-item">
        <div class="design-comment-badge">${i + 1}</div>
        <div class="design-comment-body">
          <div class="design-comment-sel">&lt;${escapeHtml(pin.elementTag)}&gt; ${escapeHtml(pin.elementSelector)}</div>
          <div class="design-comment-text">${escapeHtml(pin.comment)}</div>
        </div>
      </div>
    `).join('');
  }

  private sendAgentFeedback(): void {
    if (this.pins.length === 0 || !this.currentArtboard) return;
    const bullets = this.pins.map((p, i) => `${i + 1}. Element \`<${p.elementSelector}>\`: ${p.comment}`).join('\n');
    const message = `Please update the design in \`.pandacode/design/${this.currentArtboard.filename}\` based on this visual feedback:\n\n${bullets}`;
    this.onSendFeedback?.(message);
    toast(`Sent ${this.pins.length} visual design feedback points to agent prompt.`);
    this.pins = [];
    this.renderPins();
    this.renderCommentsList();
  }

  private showAddCommentPinModal(selector: string, tag: string, xPercent: number, yPercent: number, xPx?: number, yPx?: number): void {
    if (!this.currentArtboard) return;
    const existingModal = document.querySelector('.design-pin-comment-modal');
    if (existingModal) existingModal.remove();

    const overlay = document.createElement('div');
    overlay.className = 'design-new-artboard-modal design-pin-comment-modal';
    overlay.innerHTML = `
      <div class="design-modal-backdrop"></div>
      <div class="design-modal-box" style="max-width:380px;">
        <div class="design-modal-header">
          <div style="display:flex;align-items:center;gap:6px;">
            <span style="color:var(--accent);">${icon('pin')}</span>
            <h3>Add Visual Comment</h3>
          </div>
          <button type="button" class="icon-button" data-modal-close aria-label="Close">${icon('close')}</button>
        </div>
        <p style="font-size:11px;color:var(--muted);margin:0 0 10px;font-family:var(--font-mono, monospace);">
          &lt;${escapeHtml(selector)}&gt;
        </p>
        <form class="design-modal-form">
          <input type="text" class="design-modal-input" placeholder="e.g. Increase padding, fix font weight, align left..." required autofocus />
          <div class="design-modal-footer">
            <button type="button" class="design-modal-btn cancel" data-modal-close>Cancel</button>
            <button type="submit" class="design-modal-btn confirm">Add Pin</button>
          </div>
        </form>
      </div>
    `;

    document.body.appendChild(overlay);
    const input = overlay.querySelector<HTMLInputElement>('.design-modal-input');
    if (input) setTimeout(() => input.focus(), 20);

    const closeModal = () => overlay.remove();
    overlay.querySelectorAll('[data-modal-close], .design-modal-backdrop').forEach(el => {
      el.addEventListener('click', closeModal);
    });

    let submitted = false;
    const form = overlay.querySelector<HTMLFormElement>('.design-modal-form');
    form?.addEventListener('submit', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (submitted) return;
      const val = input ? input.value.trim() : '';
      if (!val) return;
      submitted = true;
      closeModal();
      const pin: DesignCommentPin = {
        id: `pin-${Date.now()}`,
        artboardFilename: this.currentArtboard!.filename,
        xPercent,
        yPercent,
        xPx,
        yPx,
        elementSelector: selector,
        elementTag: tag,
        comment: val,
      };
      this.pins.push(pin);
      this.renderPins();
      this.renderCommentsList();
      toast(`Pinned comment on <${selector}>`);
    });
  }

  private promptAiRemix(): void {
    this.showAiRemixModal();
  }

  private showAiRemixModal(): void {
    if (!this.currentArtboard) {
      toast('Open or select an artboard first.');
      return;
    }

    const existingModal = document.querySelector('.design-remix-modal');
    if (existingModal) existingModal.remove();

    const overlay = document.createElement('div');
    overlay.className = 'design-new-artboard-modal design-remix-modal';
    overlay.innerHTML = `
      <div class="design-modal-backdrop"></div>
      <div class="design-modal-box design-remix-box">
        <div class="design-modal-header">
          <div style="display:flex;align-items:center;gap:8px;">
            <span style="color:var(--accent);">${icon('spark')}</span>
            <h3>AI Remix Artboard</h3>
          </div>
          <button type="button" class="icon-button" data-modal-close aria-label="Close">${icon('close')}</button>
        </div>
        <p style="font-size:12px;color:var(--muted);margin:0 0 12px;">
          Describe how Swarm should remix or restyle <strong>${escapeHtml(this.currentArtboard.name)}</strong>.
        </p>
        <form class="design-modal-form">
          <textarea class="design-modal-textarea" rows="3" placeholder="e.g. Redesign into a modern dark mode dashboard with sleek frosted glass cards, glow accents, and responsive layout..." required autofocus></textarea>
          
          <div class="design-remix-presets">
            <span class="design-remix-presets-label">Quick presets:</span>
            <div class="design-remix-chips">
              <button type="button" class="design-remix-chip" data-prompt="Redesign with dark mode, deep slate backgrounds, and vivid accent glows">Dark Mode</button>
              <button type="button" class="design-remix-chip" data-prompt="Restyle into a modern glassmorphism UI with frosted glass borders and subtle backdrop blurs">Glassmorphism</button>
              <button type="button" class="design-remix-chip" data-prompt="Refine typography with large modern headlines, elegant leading, and crisp hierarchy">Modern Typography</button>
              <button type="button" class="design-remix-chip" data-prompt="Optimize layout to be fully responsive for mobile, tablet, and desktop screens with fluid flexbox">Mobile Responsive</button>
              <button type="button" class="design-remix-chip" data-prompt="Add hover micro-interactions, sleek button press effects, and smooth card transitions">Micro-Interactions</button>
            </div>
          </div>

          <div class="design-modal-footer">
            <button type="button" class="design-modal-btn cancel" data-modal-close>Cancel</button>
            <button type="submit" class="design-modal-btn confirm">
              ${icon('spark')} <span>Remix with AI</span>
            </button>
          </div>
        </form>
      </div>
    `;

    document.body.appendChild(overlay);
    const textarea = overlay.querySelector<HTMLTextAreaElement>('.design-modal-textarea');
    if (textarea) setTimeout(() => textarea.focus(), 20);

    const closeModal = () => overlay.remove();
    overlay.querySelectorAll('[data-modal-close], .design-modal-backdrop').forEach(el => {
      el.addEventListener('click', closeModal);
    });

    overlay.querySelectorAll<HTMLButtonElement>('.design-remix-chip').forEach(chip => {
      chip.addEventListener('click', () => {
        if (textarea && chip.dataset.prompt) {
          textarea.value = chip.dataset.prompt;
          textarea.focus();
        }
      });
    });

    let submitted = false;
    const form = overlay.querySelector<HTMLFormElement>('.design-modal-form');
    form?.addEventListener('submit', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (submitted) return;
      const val = textarea ? textarea.value.trim() : '';
      if (!val) return;
      submitted = true;
      closeModal();
      const message = `Please remix and redesign \`.pandacode/design/${this.currentArtboard!.filename}\`:\n\n${val}`;
      this.onSendFeedback?.(message);
      toast('Remix prompt sent to composer!');
    });

    textarea?.addEventListener('keydown', (e) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
        e.preventDefault();
        form?.requestSubmit();
      }
    });
  }

  private promptConvertCode(): void {
    if (!this.currentArtboard) return;
    const message = `Please convert the HTML design artboard at \`.pandacode/design/${this.currentArtboard.filename}\` into production React + Tailwind components in our project. Match all layouts, typography, and styling precisely.`;
    this.onSendFeedback?.(message);
    toast('Sent component conversion request to composer!');
  }

  private showNewArtboardModal(): void {
    const existingModal = document.querySelector('.design-new-artboard-modal');
    if (existingModal) existingModal.remove();

    const overlay = document.createElement('div');
    overlay.className = 'design-new-artboard-modal';
    overlay.innerHTML = `
      <div class="design-modal-backdrop"></div>
      <div class="design-modal-box">
        <div class="design-modal-header">
          <div style="display:flex;align-items:center;gap:8px;">
            <span style="color:var(--accent);">${icon('plus')}</span>
            <h3>Create New Artboard</h3>
          </div>
          <button type="button" class="icon-button" data-modal-close aria-label="Close">${icon('close')}</button>
        </div>
        <p style="font-size:12px;color:var(--muted);margin:0 0 12px;">Enter an identifier for your new HTML design canvas.</p>
        <form class="design-modal-form">
          <input type="text" class="design-modal-input" placeholder="e.g. pricing-table, profile-card" required autofocus />
          <div class="design-modal-footer">
            <button type="button" class="design-modal-btn cancel" data-modal-close>Cancel</button>
            <button type="submit" class="design-modal-btn confirm">Create Artboard</button>
          </div>
        </form>
      </div>
    `;

    document.body.appendChild(overlay);
    const input = overlay.querySelector<HTMLInputElement>('.design-modal-input');
    if (input) {
      setTimeout(() => input.focus(), 20);
    }

    const closeModal = () => overlay.remove();
    overlay.querySelectorAll('[data-modal-close], .design-modal-backdrop').forEach(el => {
      el.addEventListener('click', closeModal);
    });

    let submitted = false;
    const form = overlay.querySelector<HTMLFormElement>('.design-modal-form');
    form?.addEventListener('submit', (e) => {
      e.preventDefault();
      e.stopPropagation();
      if (submitted) return;
      const val = input ? input.value.trim() : '';
      if (!val) return;
      submitted = true;
      closeModal();
      void this.createArtboardPreset('blank', val);
    });
  }

  private async createArtboardPreset(preset: 'dashboard' | 'mobile' | 'modal' | 'blank', customName?: string): Promise<void> {
    const name = customName || (preset === 'dashboard' ? 'saas-dashboard' : preset === 'mobile' ? 'mobile-screen' : preset === 'modal' ? 'auth-dialog' : 'new-canvas');
    let template = '';
    
    if (preset === 'dashboard') {
      template = `<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Dashboard</title><style>body{font-family:system-ui,-apple-system,sans-serif;margin:0;display:flex;height:100vh;background:#0f172a;color:#f8fafc}.sidebar{width:240px;background:#1e293b;padding:24px;border-right:1px solid #334155}.nav-item{padding:10px 14px;border-radius:8px;margin-bottom:6px;cursor:pointer;background:#334155}.main{flex:1;padding:32px;overflow-y:auto}.header{display:flex;justify-content:space-between;margin-bottom:32px}.card{background:#1e293b;padding:20px;border-radius:12px;border:1px solid #334155;margin-bottom:20px}</style></head><body><div class="sidebar"><h3>PandaCloud</h3><div class="nav-item">Overview</div><div class="nav-item">Workflows</div><div class="nav-item">Settings</div></div><div class="main"><div class="header"><h2>Analytics Overview</h2><button style="background:#38bdf8;color:#0f172a;border:none;padding:8px 16px;border-radius:6px;font-weight:600;">+ New Event</button></div><div class="card"><h4>Active Nodes</h4><p style="font-size:32px;font-weight:700;margin:10px 0;color:#38bdf8;">1,428</p></div></div></body></html>`;
    } else if (preset === 'mobile') {
      template = `<!DOCTYPE html><html><head><meta charset="UTF-8"><title>Mobile Screen</title><style>body{font-family:system-ui,-apple-system,sans-serif;margin:0;padding:20px;background:#f8fafc;color:#0f172a}.card{background:#fff;border-radius:16px;padding:20px;box-shadow:0 10px 25px -5px rgb(0 0 0 / 0.1);margin-bottom:16px}.btn{width:100%;background:#0f172a;color:#fff;border:none;padding:14px;border-radius:12px;font-weight:600;font-size:16px;cursor:pointer}</style></head><body><div class="card"><span style="font-size:12px;color:#64748b;">Daily Pulse</span><h2 style="margin:8px 0;">Design Sprint</h2><p style="color:#475569;font-size:14px;">Review wireframes with the product design team.</p><button class="btn">Start Session</button></div></body></html>`;
    }

    try {
      const created = await call<DesignArtboard>('design_create_artboard', {
        sessionId: this.sessionId,
        name,
        htmlContent: template,
      });
      toast(`Created artboard ${created.name}`);
      await this.loadArtboards();
      this.currentArtboard = created;
      this.render();
      await this.loadCurrentArtboardContent();
    } catch (e) {
      toast(`Failed to create artboard: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  private async duplicateArtboard(sourceFile: string): Promise<void> {
    const baseName = sourceFile.replace(/\.html?$/i, '');
    const newName = `${baseName}-copy`;
    try {
      const dup = await call<DesignArtboard>('design_duplicate_artboard', {
        sessionId: this.sessionId,
        sourceFilename: sourceFile,
        newName,
      });
      toast(`Duplicated artboard to ${dup.name}`);
      await this.loadArtboards();
      this.currentArtboard = dup;
      this.render();
      await this.loadCurrentArtboardContent();
    } catch (e) {
      toast(`Failed to duplicate artboard: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  private async deleteArtboard(filename: string): Promise<void> {
    if (!confirm(`Delete artboard "${filename}"?`)) return;
    try {
      await call('design_delete_artboard', { sessionId: this.sessionId, filename });
      toast(`Deleted artboard ${filename}`);
      await this.loadArtboards();
    } catch (e) {
      toast(`Failed to delete artboard: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  private bindEmptyStateEvents(): void {
    this.eventsAbort?.abort();
    this.eventsAbort = new AbortController();
    const { signal } = this.eventsAbort;
    this.container?.querySelectorAll<HTMLButtonElement>('[data-create-preset]').forEach(btn => {
      btn.addEventListener('click', () => {
        const preset = btn.dataset.createPreset as 'dashboard' | 'mobile' | 'modal' | 'blank';
        void this.createArtboardPreset(preset);
      }, { signal });
    });
  }

  private rgbToHex(rgbStr: string): string {
    const match = rgbStr.match(/\d+/g);
    if (!match || match.length < 3) return '#000000';
    const r = Number(match[0]).toString(16).padStart(2, '0');
    const g = Number(match[1]).toString(16).padStart(2, '0');
    const b = Number(match[2]).toString(16).padStart(2, '0');
    return `#${r}${g}${b}`;
  }
}
