import './attachment-preview.css';
import { call, fileAssetUrl } from './bridge.ts';
import { escape, icon, toast } from './dom.ts';
import type { PromptAttachment, PromptImage, PromptDocument, PromptFolder, PromptVideo } from './attachments.ts';

const IMAGE_EXTS = /\.(png|jpe?g|webp|gif|svg|bmp|ico)$/i;
const VIDEO_EXTS = /\.(mp4|webm|mov|mkv|ogg)$/i;

function baseName(path: string): string {
  return path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

export async function openAttachmentPreview(
  target: PromptAttachment | { kind: string; [k: string]: any } | { path: string; name?: string; text?: string; size?: number }
): Promise<void> {
  const overlay = document.createElement('div');
  overlay.className = 'attachment-preview-overlay';
  overlay.setAttribute('role', 'dialog');
  overlay.setAttribute('aria-modal', 'true');

  let title = 'Attachment';
  let meta = '';
  let iconName = 'file';
  let bodyHtml = '<div style="color:var(--fg-dim,#94a3b8);">Loading preview...</div>';
  let copyContent: string | null = null;

  if ('kind' in target) {
    if (target.kind === 'image' || target.kind === 'screenshot') {
      const img = target as any;
      title = img.name || img.title || 'Screenshot';
      meta = img.width && img.height ? `${img.width} × ${img.height} px` : 'Image attachment';
      iconName = 'images';
      const src = img.dataUrl || (img.thumbnail && img.thumbnail.startsWith('data:') ? img.thumbnail : (img.path ? fileAssetUrl(img.path) : (img.thumbnail || '')));
      bodyHtml = `<img class="attachment-preview-image" src="${escape(src)}" alt="${escape(title)}">`;
    } else if (target.kind === 'video') {
      const vid = target as PromptVideo;
      title = vid.name;
      meta = vid.size ? formatBytes(vid.size) : 'Video';
      iconName = 'play';
      const src = vid.dataUrl || (vid.path ? fileAssetUrl(vid.path) : '');
      bodyHtml = `<video class="attachment-preview-video" controls autoplay loop playsinline src="${escape(src)}"></video>`;
    } else if (target.kind === 'document') {
      const doc = target as PromptDocument;
      title = doc.name;
      meta = `${formatBytes(doc.size)}${doc.truncated ? ' · first 32 KB' : ''}`;
      iconName = 'file';
      copyContent = doc.text;
      bodyHtml = `
        <div class="attachment-preview-code-wrap">
          <pre class="attachment-preview-code"><code>${escape(doc.text)}</code></pre>
        </div>
      `;
    } else if (target.kind === 'folder') {
      const fol = target as PromptFolder;
      title = `${fol.name}/`;
      meta = `${fol.files} files · ${fol.folders} folders`;
      iconName = 'folder';
      copyContent = fol.listing;
      bodyHtml = `
        <div class="attachment-preview-code-wrap">
          <pre class="attachment-preview-code"><code>${escape(fol.listing)}</code></pre>
        </div>
      `;
    }
  } else {
    // Project file path
    const filePath = target.path;
    const name = target.name || baseName(filePath);
    title = name;

    if (IMAGE_EXTS.test(filePath)) {
      iconName = 'images';
      meta = 'Image';
      const src = fileAssetUrl(filePath);
      bodyHtml = `<img class="attachment-preview-image" src="${escape(src)}" alt="${escape(name)}">`;
    } else if (VIDEO_EXTS.test(filePath)) {
      iconName = 'play';
      meta = 'Video';
      const src = fileAssetUrl(filePath);
      bodyHtml = `<video class="attachment-preview-video" controls autoplay loop playsinline src="${escape(src)}"></video>`;
    } else {
      // Text or code file
      iconName = 'file';
      try {
        let content = target.text;
        if (!content) {
          content = await call<string>('read_file', { path: filePath }).catch(() => '');
        }
        if (!content) {
          const doc = await call<Omit<PromptDocument, 'kind' | 'id'>>('read_document', { path: filePath }).catch(() => null);
          if (doc) content = doc.text;
        }
        copyContent = content || '(Empty file or unavailable content)';
        meta = `${formatBytes(target.size || copyContent.length)}`;
        bodyHtml = `
          <div class="attachment-preview-code-wrap">
            <pre class="attachment-preview-code"><code>${escape(copyContent)}</code></pre>
          </div>
        `;
      } catch (err: any) {
        bodyHtml = `<div style="color:#ef4444;">Could not read file: ${escape(String(err))}</div>`;
      }
    }
  }

  const copyButtonHtml = copyContent != null
    ? `<button type="button" class="attachment-preview-copy-btn" id="preview-copy-btn">${icon('file')}<span>Copy Text</span></button>`
    : '';

  overlay.innerHTML = `
    <div class="attachment-preview-modal">
      <div class="attachment-preview-header">
        <div class="attachment-preview-title">
          ${icon(iconName)}
          <span>${escape(title)}</span>
          <small class="attachment-preview-meta">${escape(meta)}</small>
        </div>
        <div class="attachment-preview-actions">
          ${copyButtonHtml}
          <button type="button" class="icon-button" data-action="close" aria-label="Close preview">${icon('close')}</button>
        </div>
      </div>
      <div class="attachment-preview-body">
        ${bodyHtml}
      </div>
    </div>
  `;

  document.body.appendChild(overlay);

  const close = () => {
    overlay.remove();
    document.removeEventListener('keydown', onKeyDown);
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  };
  document.addEventListener('keydown', onKeyDown);

  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) close();
  });

  overlay.querySelector('[data-action="close"]')?.addEventListener('click', close);

  const copyBtn = overlay.querySelector('#preview-copy-btn') as HTMLButtonElement;
  if (copyBtn && copyContent) {
    copyBtn.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(copyContent!);
        copyBtn.classList.add('copied');
        copyBtn.innerHTML = `${icon('check')} <span>Copied!</span>`;
        toast('Content copied to clipboard');
        setTimeout(() => {
          copyBtn.classList.remove('copied');
          copyBtn.innerHTML = `${icon('file')} <span>Copy Text</span>`;
        }, 2000);
      } catch {
        toast('Failed to copy text');
      }
    });
  }
}
