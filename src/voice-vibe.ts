import { icon, toast } from './dom.ts';

export interface VoiceVibeController {
  isRecording(): boolean;
  toggle(): Promise<void>;
  start(): Promise<void>;
  stop(): void;
  mount(container: HTMLElement, promptGetter: () => HTMLTextAreaElement | null): void;
  destroy(): void;
}

export function createVoiceVibe(): VoiceVibeController {
  let recording = false;
  let recognition: any = null;
  let audioContext: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let mediaStream: MediaStream | null = null;
  let animFrame = 0;
  let waveCanvas: HTMLCanvasElement | null = null;
  let btn: HTMLButtonElement | null = null;
  let promptRef: () => HTMLTextAreaElement | null = () => null;

  function isSpeechSupported(): boolean {
    return 'SpeechRecognition' in window || 'webkitSpeechRecognition' in window;
  }

  async function startAudioVisualizer(): Promise<void> {
    try {
      mediaStream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const AudioCtx = window.AudioContext || (window as any).webkitAudioContext;
      audioContext = new AudioCtx();
      const source = audioContext.createMediaStreamSource(mediaStream);
      analyser = audioContext.createAnalyser();
      analyser.fftSize = 32;
      source.connect(analyser);

      drawWave();
    } catch {
      // Visualizer fails gracefully if microphone media stream is blocked
    }
  }

  function stopAudioVisualizer(): void {
    cancelAnimationFrame(animFrame);
    if (mediaStream) {
      mediaStream.getTracks().forEach(track => track.stop());
      mediaStream = null;
    }
    if (audioContext && audioContext.state !== 'closed') {
      audioContext.close().catch(() => {});
      audioContext = null;
    }
    analyser = null;
    if (waveCanvas) {
      const ctx = waveCanvas.getContext('2d');
      if (ctx) ctx.clearRect(0, 0, waveCanvas.width, waveCanvas.height);
    }
  }

  function drawWave(): void {
    if (!recording || !waveCanvas || !analyser) return;
    const ctx = waveCanvas.getContext('2d');
    if (!ctx) return;

    const buffer = new Uint8Array(analyser.frequencyBinCount);
    analyser.getByteFrequencyData(buffer);

    const w = waveCanvas.width;
    const h = waveCanvas.height;
    ctx.clearRect(0, 0, w, h);

    const bars = 5;
    const barWidth = 6;
    const gap = 4;
    const startX = (w - (bars * barWidth + (bars - 1) * gap)) / 2;

    for (let i = 0; i < bars; i++) {
      const val = buffer[i * 2] || 0;
      const percent = Math.min(1, Math.max(0.15, val / 255));
      const barHeight = Math.max(3, percent * h);
      const x = startX + i * (barWidth + gap);
      const y = (h - barHeight) / 2;

      ctx.fillStyle = i % 2 === 0 ? 'var(--accent, #ffd633)' : '#ef4444';
      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 3);
      ctx.fill();
    }

    animFrame = requestAnimationFrame(drawWave);
  }

  function insertTextAtCursor(textarea: HTMLTextAreaElement, text: string): void {
    const start = textarea.selectionStart ?? textarea.value.length;
    const end = textarea.selectionEnd ?? textarea.value.length;
    const before = textarea.value.substring(0, start);
    const after = textarea.value.substring(end);
    const spacePrefix = before.length > 0 && !before.endsWith(' ') && !before.endsWith('\n') ? ' ' : '';
    const insertion = spacePrefix + text;

    textarea.value = before + insertion + after;
    textarea.selectionStart = textarea.selectionEnd = start + insertion.length;
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
  }

  async function start(): Promise<void> {
    if (recording) return;

    if (!isSpeechSupported()) {
      toast('Speech recognition is not supported in this environment.');
      return;
    }

    try {
      const Speech = (window as any).SpeechRecognition || (window as any).webkitSpeechRecognition;
      recognition = new Speech();
      recognition.continuous = true;
      recognition.interimResults = false;
      recognition.lang = navigator.language || 'en-US';

      recognition.onresult = (event: any) => {
        let finalTranscript = '';
        for (let i = event.resultIndex; i < event.results.length; ++i) {
          if (event.results[i].isFinal) {
            finalTranscript += event.results[i][0].transcript;
          }
        }
        if (finalTranscript.trim()) {
          const textarea = promptRef();
          if (textarea) {
            insertTextAtCursor(textarea, finalTranscript.trim());
          }
        }
      };

      recognition.onerror = () => {
        stop();
      };

      recognition.onend = () => {
        if (recording) {
          try {
            recognition.start();
          } catch {
            stop();
          }
        }
      };

      recognition.start();
      recording = true;
      if (btn) {
        btn.classList.add('recording');
        btn.setAttribute('aria-pressed', 'true');
        btn.title = 'Stop dictation (Click or release F8)';
      }
      if (waveCanvas) waveCanvas.parentElement?.removeAttribute('hidden');

      await startAudioVisualizer();
    } catch {
      stop();
    }
  }

  function stop(): void {
    isVoiceActive = false;
    if (!recording) return;
    recording = false;
    if (recognition) {
      try {
        recognition.stop();
      } catch {
        // Ignore
      }
      recognition = null;
    }
    stopAudioVisualizer();
    if (btn) {
      btn.classList.remove('recording');
      btn.setAttribute('aria-pressed', 'false');
      btn.title = 'Dictate prompt (Hold F8 or click)';
    }
    if (waveCanvas) waveCanvas.parentElement?.setAttribute('hidden', '');
  }

  async function toggle(): Promise<void> {
    if (recording) {
      stop();
    } else {
      await start();
    }
  }

  // Hotkey push-to-talk handler (F8)
  let isVoiceActive = false;
  function onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'F8') {
      e.preventDefault();
      if (e.repeat || isVoiceActive || recording) return;
      isVoiceActive = true;
      void start();
    }
  }

  function onKeyUp(e: KeyboardEvent): void {
    if (e.key === 'F8') {
      e.preventDefault();
      if (!isVoiceActive && !recording) return;
      isVoiceActive = false;
      stop();
    }
  }

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);

  return {
    isRecording: () => recording,
    toggle,
    start,
    stop,
    mount(container: HTMLElement, promptGetter: () => HTMLTextAreaElement | null) {
      promptRef = promptGetter;

      const wrap = document.createElement('div');
      wrap.style.display = 'inline-flex';
      wrap.style.alignItems = 'center';
      wrap.style.gap = '4px';

      wrap.innerHTML = `
        <button type="button" class="voice-vibe-btn" id="voice-dictate-btn" aria-label="Voice dictation" title="Dictate prompt (Hold F8 or click)">
          ${icon('mic')}
        </button>
        <div class="voice-wave-container" hidden>
          <canvas class="voice-wave-canvas" width="56" height="18"></canvas>
        </div>
      `;

      container.appendChild(wrap);
      btn = wrap.querySelector('#voice-dictate-btn');
      waveCanvas = wrap.querySelector('.voice-wave-canvas');

      btn?.addEventListener('click', e => {
        e.preventDefault();
        e.stopPropagation();
        void toggle();
      });
    },
    destroy() {
      stop();
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    },
  };
}
