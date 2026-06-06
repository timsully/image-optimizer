<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import FileCard from "$lib/FileCard.svelte";
  import type { ConvertProgressPayload, ConvertResultPayload, FileItem, Stage } from "$lib/types";

  const IMAGE_EXTENSIONS = new Set([
    "png",
    "jpg",
    "jpeg",
    "webp",
    "gif",
    "bmp",
    "tiff",
    "tif",
    "heic",
    "heif",
  ]);

  const FORMATS = [
    { value: "png", label: "PNG" },
    { value: "jpeg", label: "JPEG" },
    { value: "webp", label: "WebP" },
    { value: "tiff", label: "TIFF" },
    { value: "bmp", label: "BMP" },
  ];

  let targetFormat = $state("png");
  let items = $state<FileItem[]>([]);
  let isDraggingOver = $state(false);
  let toast = $state<{ visible: boolean; text: string }>({ visible: false, text: "" });
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  let nextId = 0;
  function makeId() {
    nextId += 1;
    return `f${Date.now()}-${nextId}`;
  }

  function basename(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function extOf(name: string): string {
    const dot = name.lastIndexOf(".");
    return dot === -1 ? "" : name.slice(dot + 1).toLowerCase();
  }

  function updateItem(id: string, patch: Partial<FileItem>) {
    items = items.map((it) => (it.id === id ? { ...it, ...patch } : it));
  }

  async function processFile(item: FileItem) {
    updateItem(item.id, { stage: "decoding", percent: 10 });
    try {
      const result = await invoke<ConvertResultPayload>("convert_image", {
        request: {
          id: item.id,
          source_path: item.path,
          target_format: targetFormat,
          quality: 85,
        },
      });
      updateItem(item.id, {
        stage: "done",
        percent: 100,
        outputPath: result.output_path,
        originalBytes: result.original_bytes,
        outputBytes: result.output_bytes,
      });
    } catch (err) {
      updateItem(item.id, { stage: "error", error: String(err) });
    }
    maybeAnnounceBatchDone();
  }

  function maybeAnnounceBatchDone() {
    if (items.length === 0) return;
    const allSettled = items.every((it) => it.stage === "done" || it.stage === "error");
    if (!allSettled) return;

    const succeeded = items.filter((it) => it.stage === "done").length;
    const failed = items.filter((it) => it.stage === "error").length;
    const text =
      failed === 0
        ? `Converted ${succeeded} ${succeeded === 1 ? "image" : "images"} to ${targetFormat.toUpperCase()}`
        : `Converted ${succeeded}, ${failed} failed`;

    showToast(text);
  }

  function showToast(text: string) {
    toast = { visible: true, text };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = { ...toast, visible: false };
    }, 3200);
  }

  async function ingestPaths(paths: string[]) {
    const imagePaths = paths.filter((p) => IMAGE_EXTENSIONS.has(extOf(p)));
    if (imagePaths.length === 0) return;

    const newItems: FileItem[] = imagePaths.map((path) => ({
      id: makeId(),
      path,
      name: basename(path),
      ext: extOf(path),
      stage: "queued" as Stage,
      percent: 0,
    }));

    items = [...items, ...newItems];

    // Stagger kickoff so the "pop + settle" entrance cascades, then process
    // concurrently — each card animates its own shimmer/progress independently.
    newItems.forEach((item, i) => {
      setTimeout(() => processFile(item), i * 60);
    });
  }

  function clearAll() {
    items = [];
    toast = { visible: false, text: "" };
  }

  async function browseForFiles() {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Images", extensions: [...IMAGE_EXTENSIONS] }],
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    ingestPaths(paths);
  }

  onMount(() => {
    let unlistenDrop: (() => void) | undefined;
    let unlistenProgress: (() => void) | undefined;

    (async () => {
      const webview = getCurrentWebviewWindow();
      unlistenDrop = await webview.onDragDropEvent((event) => {
        const payload = event.payload as { type: string; paths?: string[] };
        if (payload.type === "enter" || payload.type === "over") {
          isDraggingOver = true;
        } else if (payload.type === "drop") {
          isDraggingOver = false;
          if (payload.paths) ingestPaths(payload.paths);
        } else if (payload.type === "leave") {
          isDraggingOver = false;
        }
      });

      unlistenProgress = await listen<ConvertProgressPayload>("convert://progress", (event) => {
        const { id, stage, percent } = event.payload;
        updateItem(id, { stage, percent });
      });
    })();

    return () => {
      unlistenDrop?.();
      unlistenProgress?.();
    };
  });
</script>

<main class="app">
  <header class="topbar">
    <div class="brand">
      <span class="brand-mark"></span>
      <h1>Image Optimizer</h1>
    </div>

    <div class="controls">
      <label class="format-select">
        <span>Convert to</span>
        <select bind:value={targetFormat}>
          {#each FORMATS as f (f.value)}
            <option value={f.value}>{f.label}</option>
          {/each}
        </select>
      </label>
      {#if items.length > 0}
        <button class="ghost-btn" onclick={clearAll}>Clear</button>
      {/if}
    </div>
  </header>

  <section class="drop-zone" class:active={isDraggingOver} class:has-items={items.length > 0}>
    {#if items.length === 0}
      <button class="empty-state" onclick={browseForFiles} aria-label="Choose images to convert">
        <div class="drop-icon" class:hovering={isDraggingOver}>
          <svg viewBox="0 0 24 24" fill="none" aria-hidden="true">
            <path
              d="M12 16V4M12 4L7 9M12 4l5 5"
              stroke="var(--accent)"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
            <path
              d="M5 16v2a2 2 0 002 2h10a2 2 0 002-2v-2"
              stroke="var(--text-tertiary)"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </div>
        <p class="empty-title">Drop images here, or click to browse</p>
        <p class="empty-hint">HEIC, PNG, JPEG, WebP, GIF, BMP, TIFF — converted &amp; optimized on your machine</p>
      </button>
    {:else}
      <div class="file-list">
        {#each items as item, i (item.id)}
          <FileCard {item} index={i} />
        {/each}
      </div>
    {/if}
  </section>

  {#if toast.visible}
    <div class="toast">{toast.text}</div>
  {/if}
</main>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    padding: 20px 24px;
    gap: 16px;
  }

  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .brand-mark {
    width: 22px;
    height: 22px;
    border-radius: var(--radius-md);
    background: linear-gradient(135deg, var(--accent), var(--brand));
    box-shadow: var(--shadow-low);
  }

  h1 {
    font-size: var(--text-large);
    font-weight: var(--weight-semibold);
    letter-spacing: var(--tracking-tight);
    margin: 0;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .format-select {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-mini);
    color: var(--text-tertiary);
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    padding: 6px 10px;
  }

  .format-select select {
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: var(--text-small);
    font-weight: var(--weight-medium);
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }

  .format-select select option {
    background: var(--bg-secondary);
    color: var(--text-primary);
  }

  .ghost-btn {
    background: transparent;
    border: 1px solid var(--border-primary);
    color: var(--text-tertiary);
    font-size: var(--text-mini);
    font-weight: var(--weight-medium);
    border-radius: var(--radius-md);
    padding: 7px 12px;
    cursor: pointer;
    transition:
      border-color var(--duration-fade-out) ease-out,
      color var(--duration-quick) ease-in-out,
      background-color var(--duration-quick) ease-in-out;
  }

  .ghost-btn:hover {
    color: var(--text-primary);
    border-color: var(--border-secondary);
    background: var(--bg-tertiary);
  }

  .drop-zone {
    flex: 1;
    border-radius: var(--radius-xl);
    border: 1.5px dashed var(--border-primary);
    background: var(--bg-panel);
    overflow: hidden;
    display: flex;
    transition:
      border-color var(--duration-regular) var(--ease-standard),
      background-color var(--duration-regular) var(--ease-standard);
  }

  .drop-zone.active {
    border-color: var(--accent);
    background: var(--accent-tint);
  }

  .drop-zone.has-items {
    border-style: solid;
    align-items: stretch;
  }

  .empty-state {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
    padding: 32px;
    background: transparent;
    border: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
    border-radius: var(--radius-lg);
    transition: background-color var(--duration-regular) var(--ease-standard);
  }

  .empty-state:hover {
    background: var(--bg-secondary);
  }

  .empty-state:hover .drop-icon {
    transform: translateY(-3px);
    border-color: var(--border-secondary);
  }

  .drop-icon {
    width: 56px;
    height: 56px;
    border-radius: var(--radius-circle);
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: transform var(--duration-regular) var(--ease-out-quart);
  }

  .drop-icon svg {
    width: 24px;
    height: 24px;
  }

  .drop-icon.hovering {
    transform: translateY(-4px) scale(1.06);
    border-color: var(--accent);
    animation: hover-bob 1.4s var(--ease-in-out-cubic) infinite;
  }

  @keyframes hover-bob {
    0%,
    100% {
      transform: translateY(-4px) scale(1.06);
    }
    50% {
      transform: translateY(-9px) scale(1.06);
    }
  }

  .empty-title {
    font-size: var(--text-regular);
    font-weight: var(--weight-medium);
    color: var(--text-primary);
    margin: 0;
  }

  .empty-hint {
    font-size: var(--text-mini);
    color: var(--text-tertiary);
    max-width: 360px;
    margin: 0;
  }

  .file-list {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
  }

  .toast {
    position: fixed;
    left: 50%;
    bottom: 28px;
    transform: translateX(-50%);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-secondary);
    color: var(--text-primary);
    font-size: var(--text-mini);
    font-weight: var(--weight-medium);
    padding: 10px 18px;
    border-radius: var(--radius-pill);
    box-shadow: var(--shadow-medium);
    backdrop-filter: var(--blur-panel);
    animation: toast-up var(--duration-regular) var(--ease-out-quart) backwards;
  }

  @keyframes toast-up {
    0% {
      opacity: 0;
      transform: translate(-50%, 14px);
    }
    100% {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
</style>
