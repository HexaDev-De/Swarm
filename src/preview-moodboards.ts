/** Browser preview persistence. Native builds use the scoped SQLite commands. */
import type { Moodboard, MoodboardDetail, MoodboardImage } from './moodboards';

type StoredImage = MoodboardImage & { boardId: string };
type ImageData = { id: string; dataUrl: string; thumbnail: string };
let database: Promise<IDBDatabase> | undefined;
function db(): Promise<IDBDatabase> {
  return database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open('pandacode-preview-moodboards', 1);
    request.onupgradeneeded = () => {
      const database = request.result;
      database.createObjectStore('boards', { keyPath: 'id' }).createIndex('project', 'projectId');
      database.createObjectStore('images', { keyPath: 'id' }).createIndex('board', 'boardId');
      database.createObjectStore('data', { keyPath: 'id' });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error('Moodboard storage is unavailable.'));
  });
}
function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(new Error('Could not save moodboard.')); });
}
async function read<T>(store: string, id: string): Promise<T | undefined> {
  return result((await db()).transaction(store).objectStore(store).get(id));
}
async function find<T>(store: string, index: string, id: string): Promise<T[]> {
  return result((await db()).transaction(store).objectStore(store).index(index).getAll(id));
}
async function write(operations: { store: string; value?: unknown; remove?: string }[]): Promise<void> {
  const tx = (await db()).transaction([...new Set(operations.map(item => item.store))], 'readwrite');
  for (const item of operations) {
    if (item.remove !== undefined) tx.objectStore(item.store).delete(item.remove);
    else tx.objectStore(item.store).put(item.value);
  }
  await new Promise<void>((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onerror = tx.onabort = () => reject(new Error('Could not save moodboard.')); });
}
async function board(projectId: string, boardId: string): Promise<Moodboard> {
  const board = await read<Moodboard>('boards', boardId);
  if (!board || board.projectId !== projectId) throw new Error('Moodboard is unavailable in this project.');
  return board;
}
function name(value: unknown, max = 120): string {
  const text = String(value ?? '').trim();
  if (!text || text.length > max || /[\x00-\x1f]/.test(text)) throw new Error(`Use a name between 1 and ${max} characters.`);
  return text;
}
async function detail(projectId: string, boardId: string): Promise<MoodboardDetail> {
  const selected = await board(projectId, boardId);
  const images = await find<StoredImage>('images', 'board', boardId);
  return { ...selected, imageCount: images.length, images: images.map(({ boardId: _, ...image }) => image).sort((a, b) => a.createdAt - b.createdAt) };
}
async function derivatives(dataUrl: string): Promise<{ width: number; height: number; dataUrl: string; thumbnail: string }> {
  if (dataUrl.length > 12 * 1024 * 1024 * 4 / 3 + 128 || !/^data:image\/(png|jpeg|webp);base64,/.test(dataUrl)) throw new Error('Choose PNG, JPEG or WebP images up to 12 MB each.');
  const response = await fetch(dataUrl), blob = await response.blob();
  const image = await createImageBitmap(blob);
  try {
    if (image.width > 8192 || image.height > 8192 || image.width * image.height > 32_000_000) throw new Error('Choose an image smaller than 8192 pixels per edge.');
    function resize(edge: number): string {
      const scale = Math.min(1, edge / image.width, edge / image.height);
      const canvas = document.createElement('canvas'); canvas.width = Math.max(1, Math.round(image.width * scale)); canvas.height = Math.max(1, Math.round(image.height * scale));
      const context = canvas.getContext('2d')!; context.fillStyle = '#fff'; context.fillRect(0, 0, canvas.width, canvas.height); context.drawImage(image, 0, 0, canvas.width, canvas.height);
      return canvas.toDataURL('image/jpeg', .85);
    }
    return { width: image.width, height: image.height, dataUrl: resize(2048), thumbnail: resize(480) };
  } finally { image.close(); }
}
export async function invokeMoodboards<T>(command: string, args: Record<string, unknown>): Promise<T> {
  return await execute(command, args) as T;
}
async function execute(command: string, args: Record<string, unknown>): Promise<unknown> {
  const projectId = String(args.projectId ?? ''), boardId = String(args.boardId ?? ''), imageId = String(args.imageId ?? '');
  if (!projectId) throw new Error('Choose a project first.');
  if (command === 'list_moodboards') return (await find<Moodboard>('boards', 'project', projectId)).sort((a, b) => b.updatedAt - a.updatedAt);
  if (command === 'create_moodboard') {
    const boards = await find<Moodboard>('boards', 'project', projectId);
    if (boards.length >= 24) throw new Error('This project has 24 moodboards. Remove one first.');
    const item: Moodboard = { id: crypto.randomUUID(), projectId, name: name(args.name), imageCount: 0, updatedAt: Date.now() };
    await write([{ store: 'boards', value: item }]); return item;
  }
  const selected = await board(projectId, boardId);
  if (command === 'read_moodboard') return detail(projectId, boardId);
  if (command === 'rename_moodboard') {
    selected.name = name(args.name); selected.updatedAt = Date.now(); await write([{ store: 'boards', value: selected }]); return selected;
  }
  if (command === 'delete_moodboard') {
    const images = await find<StoredImage>('images', 'board', boardId);
    await write([{ store: 'boards', remove: boardId }, ...images.flatMap(image => [{ store: 'images', remove: image.id }, { store: 'data', remove: image.id }])]); return;
  }
  if (command === 'add_moodboard_image') {
    if (selected.imageCount >= 64) throw new Error('A moodboard can hold up to 64 images.');
    const prepared = await derivatives(String(args.dataUrl ?? ''));
    const image: StoredImage = { id: crypto.randomUUID(), boardId, name: name(args.name, 200), caption: '', width: prepared.width, height: prepared.height, createdAt: Date.now() };
    selected.imageCount++; selected.updatedAt = Date.now();
    await write([{ store: 'images', value: image }, { store: 'data', value: { id: image.id, dataUrl: prepared.dataUrl, thumbnail: prepared.thumbnail } }, { store: 'boards', value: selected }]);
    return detail(projectId, boardId);
  }
  if (command === 'import_moodboard_images') throw new Error('Choose image files in the browser preview.');
  const image = await read<StoredImage>('images', imageId);
  if (!image || image.boardId !== boardId) throw new Error('Image is unavailable in this moodboard.');
  if (command === 'moodboard_image') {
    const data = await read<ImageData>('data', imageId); if (!data) throw new Error('Image is no longer available.');
    return { dataUrl: args.thumbnail === false ? data.dataUrl : data.thumbnail };
  }
  if (command === 'update_moodboard_image') {
    const caption = String(args.caption ?? ''); if (caption.length > 2000 || caption.includes('\0')) throw new Error('Captions must be under 2,000 characters.');
    image.caption = caption; selected.updatedAt = Date.now(); await write([{ store: 'images', value: image }, { store: 'boards', value: selected }]); return;
  }
  if (command === 'remove_moodboard_image') {
    selected.imageCount = Math.max(0, selected.imageCount - 1); selected.updatedAt = Date.now();
    await write([{ store: 'images', remove: imageId }, { store: 'data', remove: imageId }, { store: 'boards', value: selected }]); return;
  }
  throw new Error('Unknown moodboard command.');
}
