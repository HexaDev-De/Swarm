import { icon } from './dom.ts';
import type { BrowserScreenshot } from './types.ts';

export interface LiveSketchOptions {
  host: HTMLElement;
  screenshot: BrowserScreenshot;
  onAttach(before: BrowserScreenshot, annotated: BrowserScreenshot): void;
  onClose(): void;
}

type Tool = 'arrow' | 'rect' | 'pen' | 'text';

interface Shape {
  type: Tool;
  color: string;
  weight: number;
  // For rect, arrow: start (x1, y1) and end (x2, y2)
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  // For pen:
  points?: { x: number; y: number }[];
  // For text:
  text?: string;
}

export function mountLiveSketch(options: LiveSketchOptions): { close(): void } {
  let disposed = false;
  let tool: Tool = 'arrow';
  let color = '#ef4444';
  let weight = 4;
  let shapes: Shape[] = [];
  let undoStack: Shape[][] = [];
  let currentShape: Shape | null = null;
  let bgImage: HTMLImageElement | null = null;

  const overlay = document.createElement('div');
  overlay.className = 'live-sketch-overlay';

  overlay.innerHTML = `
    <div class="live-sketch-bar">
      <div class="live-sketch-group" role="toolbar" aria-label="Drawing tools">
        <button type="button" class="live-sketch-tool active" data-sketch-tool="arrow" title="Draw Arrow">${icon('arrow-right')}</button>
        <button type="button" class="live-sketch-tool" data-sketch-tool="rect" title="Draw Box">${icon('square')}</button>
        <button type="button" class="live-sketch-tool" data-sketch-tool="pen" title="Freehand Pen">${icon('edit')}</button>
        <button type="button" class="live-sketch-tool" data-sketch-tool="text" title="Text Label">${icon('text')}</button>
      </div>

      <div class="live-sketch-group" role="toolbar" aria-label="Color presets">
        <button type="button" class="live-sketch-color active" data-color="#ef4444" style="background:#ef4444" title="Red"></button>
        <button type="button" class="live-sketch-color" data-color="#f59e0b" style="background:#f59e0b" title="Yellow"></button>
        <button type="button" class="live-sketch-color" data-color="#3b82f6" style="background:#3b82f6" title="Blue"></button>
        <button type="button" class="live-sketch-color" data-color="#10b981" style="background:#10b981" title="Green"></button>
        <button type="button" class="live-sketch-color" data-color="#ffffff" style="background:#ffffff" title="White"></button>
      </div>

      <div class="live-sketch-group" role="toolbar" aria-label="Stroke weight">
        <button type="button" class="live-sketch-tool live-sketch-weight" data-weight="2" title="Thin">Thin</button>
        <button type="button" class="live-sketch-tool live-sketch-weight active" data-weight="4" title="Medium">Med</button>
        <button type="button" class="live-sketch-tool live-sketch-weight" data-weight="8" title="Thick">Thick</button>
      </div>

      <div class="live-sketch-group">
        <button type="button" class="live-sketch-tool" data-sketch-act="undo" title="Undo">${icon('undo')}</button>
        <button type="button" class="live-sketch-tool" data-sketch-act="clear" title="Clear all">${icon('trash')}</button>
      </div>

      <div class="live-sketch-spacer"></div>

      <button type="button" class="live-sketch-btn live-sketch-cancel" data-sketch-act="cancel">Cancel</button>
      <button type="button" class="live-sketch-btn live-sketch-done" data-sketch-act="done">Attach to Prompt</button>
    </div>

    <div class="live-sketch-canvas-wrap">
      <canvas class="live-sketch-canvas"></canvas>
    </div>
  `;

  options.host.appendChild(overlay);

  const canvas = overlay.querySelector<HTMLCanvasElement>('.live-sketch-canvas')!;
  const ctx = canvas.getContext('2d')!;

  // Load screenshot
  const img = new Image();
  img.onload = () => {
    if (disposed) return;
    bgImage = img;
    fitCanvas();
  };
  img.src = options.screenshot.dataUrl;

  function fitCanvas(): void {
    if (!bgImage) return;
    canvas.width = bgImage.naturalWidth || options.screenshot.width || 1280;
    canvas.height = bgImage.naturalHeight || options.screenshot.height || 800;
    render();
  }

  function getCanvasCoords(event: MouseEvent | PointerEvent): { x: number; y: number } {
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return {
      x: (event.clientX - rect.left) * scaleX,
      y: (event.clientY - rect.top) * scaleY,
    };
  }

  function drawArrow(context: CanvasRenderingContext2D, s: Shape): void {
    const dx = s.x2 - s.x1;
    const dy = s.y2 - s.y1;
    const angle = Math.atan2(dy, dx);
    const headLen = Math.max(16, s.weight * 4);

    context.save();
    context.strokeStyle = s.color;
    context.fillStyle = s.color;
    context.lineWidth = s.weight;
    context.lineCap = 'round';
    context.lineJoin = 'round';

    // Shaft
    context.beginPath();
    context.moveTo(s.x1, s.y1);
    context.lineTo(s.x2, s.y2);
    context.stroke();

    // Arrowhead
    context.beginPath();
    context.moveTo(s.x2, s.y2);
    context.lineTo(
      s.x2 - headLen * Math.cos(angle - Math.PI / 6),
      s.y2 - headLen * Math.sin(angle - Math.PI / 6)
    );
    context.lineTo(
      s.x2 - headLen * Math.cos(angle + Math.PI / 6),
      s.y2 - headLen * Math.sin(angle + Math.PI / 6)
    );
    context.closePath();
    context.fill();
    context.restore();
  }

  function drawRect(context: CanvasRenderingContext2D, s: Shape): void {
    const x = Math.min(s.x1, s.x2);
    const y = Math.min(s.y1, s.y2);
    const w = Math.abs(s.x2 - s.x1);
    const h = Math.abs(s.y2 - s.y1);

    context.save();
    context.strokeStyle = s.color;
    context.lineWidth = s.weight;
    // Semi-transparent fill for high contrast
    context.fillStyle = s.color + '22';

    context.beginPath();
    context.roundRect(x, y, w, h, 6);
    context.fill();
    context.stroke();
    context.restore();
  }

  function drawPen(context: CanvasRenderingContext2D, s: Shape): void {
    if (!s.points || s.points.length < 2) return;
    context.save();
    context.strokeStyle = s.color;
    context.lineWidth = s.weight;
    context.lineCap = 'round';
    context.lineJoin = 'round';

    context.beginPath();
    context.moveTo(s.points[0].x, s.points[0].y);
    for (let i = 1; i < s.points.length; i++) {
      context.lineTo(s.points[i].x, s.points[i].y);
    }
    context.stroke();
    context.restore();
  }

  function drawText(context: CanvasRenderingContext2D, s: Shape): void {
    if (!s.text) return;
    context.save();
    const fontSize = Math.max(16, s.weight * 4.5);
    context.font = `bold ${fontSize}px sans-serif`;
    context.textBaseline = 'top';

    const metrics = context.measureText(s.text);
    const padding = 8;
    const textW = metrics.width;
    const textH = fontSize * 1.25;

    // Background badge
    context.fillStyle = 'rgba(15, 23, 42, 0.9)';
    context.strokeStyle = s.color;
    context.lineWidth = 2;
    context.beginPath();
    context.roundRect(s.x1 - padding, s.y1 - padding, textW + padding * 2, textH + padding * 2, 6);
    context.fill();
    context.stroke();

    // Text itself
    context.fillStyle = s.color;
    context.fillText(s.text, s.x1, s.y1);
    context.restore();
  }

  function renderShape(context: CanvasRenderingContext2D, s: Shape): void {
    switch (s.type) {
      case 'arrow':
        drawArrow(context, s);
        break;
      case 'rect':
        drawRect(context, s);
        break;
      case 'pen':
        drawPen(context, s);
        break;
      case 'text':
        drawText(context, s);
        break;
    }
  }

  function render(): void {
    if (!bgImage) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(bgImage, 0, 0, canvas.width, canvas.height);

    for (const shape of shapes) {
      renderShape(ctx, shape);
    }
    if (currentShape) {
      renderShape(ctx, currentShape);
    }
  }

  let activeTextInput: HTMLInputElement | null = null;

  function commitActiveTextInput(): void {
    if (!activeTextInput) return;
    const input = activeTextInput;
    activeTextInput = null;
    const val = input.value.trim();
    const x = Number(input.dataset.x);
    const y = Number(input.dataset.y);
    const textCol = input.dataset.color || color;
    const textWt = Number(input.dataset.weight || weight);
    input.remove();
    if (val) {
      undoStack.push([...shapes]);
      shapes.push({
        type: 'text',
        color: textCol,
        weight: textWt,
        x1: x,
        y1: y,
        x2: x,
        y2: y,
        text: val,
      });
      render();
    }
  }

  // Pointer event handling
  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 || !bgImage) return;
    e.stopPropagation();
    const coords = getCanvasCoords(e);

    if (tool === 'text') {
      if (activeTextInput) {
        commitActiveTextInput();
        return;
      }
      const canvasWrap = overlay.querySelector<HTMLElement>('.live-sketch-canvas-wrap');
      if (!canvasWrap) return;
      const wrapRect = canvasWrap.getBoundingClientRect();
      const inputX = e.clientX - wrapRect.left;
      const inputY = e.clientY - wrapRect.top;

      const input = document.createElement('input');
      input.type = 'text';
      input.className = 'live-sketch-inline-input';
      input.placeholder = 'Type text… (Enter to place)';
      input.style.left = `${Math.max(8, inputX - 10)}px`;
      input.style.top = `${Math.max(8, inputY - 14)}px`;
      input.style.borderColor = color;
      input.style.color = color;
      input.dataset.x = String(coords.x);
      input.dataset.y = String(coords.y);
      input.dataset.color = color;
      input.dataset.weight = String(weight);

      input.addEventListener('pointerdown', ev => ev.stopPropagation());
      input.addEventListener('mousedown', ev => ev.stopPropagation());
      input.addEventListener('click', ev => ev.stopPropagation());
      input.addEventListener('keydown', ev => {
        ev.stopPropagation();
        if (ev.key === 'Enter') {
          ev.preventDefault();
          commitActiveTextInput();
        } else if (ev.key === 'Escape') {
          ev.preventDefault();
          activeTextInput?.remove();
          activeTextInput = null;
        }
      });
      input.addEventListener('blur', () => {
        setTimeout(() => commitActiveTextInput(), 150);
      });

      canvasWrap.appendChild(input);
      activeTextInput = input;
      requestAnimationFrame(() => input.focus());
      return;
    }

    commitActiveTextInput();
    currentShape = {
      type: tool,
      color,
      weight,
      x1: coords.x,
      y1: coords.y,
      x2: coords.x,
      y2: coords.y,
      points: tool === 'pen' ? [coords] : undefined,
    };
    canvas.setPointerCapture(e.pointerId);
    render();
  }

  function onPointerMove(e: PointerEvent): void {
    if (!currentShape) return;
    e.stopPropagation();
    const coords = getCanvasCoords(e);
    currentShape.x2 = coords.x;
    currentShape.y2 = coords.y;
    if (currentShape.type === 'pen' && currentShape.points) {
      currentShape.points.push(coords);
    }
    render();
  }

  function onPointerUp(e: PointerEvent): void {
    if (!currentShape) return;
    e.stopPropagation();
    try {
      canvas.releasePointerCapture(e.pointerId);
    } catch {
      // Ignore if not captured
    }
    undoStack.push([...shapes]);
    shapes.push(currentShape);
    currentShape = null;
    render();
  }

  canvas.addEventListener('pointerdown', onPointerDown);
  canvas.addEventListener('pointermove', onPointerMove);
  canvas.addEventListener('pointerup', onPointerUp);

  // Toolbar events
  function onToolbarClick(e: MouseEvent): void {
    e.stopPropagation();
    const target = (e.target as Element).closest<HTMLElement>('button');
    if (!target) return;

    if (target.dataset.sketchTool) {
      commitActiveTextInput();
      tool = target.dataset.sketchTool as Tool;
      overlay.querySelectorAll('[data-sketch-tool]').forEach(b => b.classList.remove('active'));
      target.classList.add('active');
    } else if (target.dataset.color) {
      color = target.dataset.color;
      overlay.querySelectorAll('[data-color]').forEach(b => b.classList.remove('active'));
      target.classList.add('active');
      if (activeTextInput) {
        activeTextInput.style.borderColor = color;
        activeTextInput.style.color = color;
        activeTextInput.dataset.color = color;
      }
    } else if (target.dataset.weight) {
      weight = Number(target.dataset.weight);
      overlay.querySelectorAll('[data-weight]').forEach(b => b.classList.remove('active'));
      target.classList.add('active');
    } else if (target.dataset.sketchAct === 'undo') {
      commitActiveTextInput();
      if (undoStack.length > 0) {
        shapes = undoStack.pop()!;
        render();
      }
    } else if (target.dataset.sketchAct === 'clear') {
      commitActiveTextInput();
      if (shapes.length > 0) {
        undoStack.push([...shapes]);
        shapes = [];
        render();
      }
    } else if (target.dataset.sketchAct === 'cancel') {
      close();
    } else if (target.dataset.sketchAct === 'done') {
      commitActiveTextInput();
      exportAndAttach();
    }
  }

  overlay.addEventListener('click', onToolbarClick);
  overlay.querySelector('.live-sketch-bar')?.addEventListener('pointerdown', e => e.stopPropagation());

  function exportAndAttach(): void {
    if (!bgImage) return;

    // Export composite image
    const annotatedDataUrl = canvas.toDataURL('image/png');

    const before: BrowserScreenshot = {
      ...options.screenshot,
      id: crypto.randomUUID(),
      name: 'before-preview.png',
    };

    const annotated: BrowserScreenshot = {
      ...options.screenshot,
      id: crypto.randomUUID(),
      name: 'annotated-preview.png',
      dataUrl: annotatedDataUrl,
    };

    options.onAttach(before, annotated);
    close();
  }

  function close(): void {
    if (disposed) return;
    disposed = true;
    commitActiveTextInput();
    canvas.removeEventListener('pointerdown', onPointerDown);
    canvas.removeEventListener('pointermove', onPointerMove);
    canvas.removeEventListener('pointerup', onPointerUp);
    overlay.removeEventListener('click', onToolbarClick);
    overlay.remove();
    options.onClose();
  }

  return { close };
}
