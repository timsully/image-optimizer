<script lang="ts">
  import { save } from "@tauri-apps/plugin-dialog";
  import { invoke } from "@tauri-apps/api/core";
  import type { FileItem } from "./types";

  let { item, index }: { item: FileItem; index: number } = $props();

  function formatBytes(bytes?: number): string {
    if (bytes === undefined) return "—";
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  }

  function savings(item: FileItem): string | null {
    if (item.originalBytes === undefined || item.outputBytes === undefined) return null;
    if (item.originalBytes === 0) return null;
    const pct = Math.round((1 - item.outputBytes / item.originalBytes) * 100);
    if (pct <= 0) return null;
    return `−${pct}%`;
  }

  const stageLabel: Record<string, string> = {
    queued: "Queued",
    decoding: "Decoding",
    encoding: "Converting",
    optimizing: "Optimizing",
    done: "Done",
    error: "Failed",
  };

  let entranceDelay = $derived(`${index * 50}ms`);
  let isActive = $derived(item.stage !== "done" && item.stage !== "error");

  async function saveImage() {
    if (!item.outputPath) return;
    const ext = item.outputPath.split(".").pop() ?? "png";
    const stem = item.name.replace(/\.[^.]+$/, "");
    const dest = await save({
      defaultPath: `${stem}.${ext}`,
      filters: [{ name: "Image", extensions: [ext] }],
    });
    if (dest) {
      await invoke("save_image_to", { sourcePath: item.outputPath, destPath: dest });
    }
  }
</script>

<div
  class="card"
  class:done={item.stage === "done"}
  class:error={item.stage === "error"}
  style:animation-delay={entranceDelay}
>
  <div class="thumb" class:scanning={isActive}>
    {#if isActive}
      <div class="shimmer"></div>
      <span class="ext">.{item.ext}</span>
    {:else if item.stage === "done"}
      <svg class="checkmark" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M4 12.5L9.5 18L20 6"
          stroke="var(--success)"
          stroke-width="2.5"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
    {:else}
      <svg class="error-mark" viewBox="0 0 24 24" fill="none" aria-hidden="true">
        <path
          d="M6 6L18 18M6 18L18 6"
          stroke="var(--danger)"
          stroke-width="2.5"
          stroke-linecap="round"
        />
      </svg>
    {/if}
  </div>

  <div class="meta">
    <div class="name" title={item.name}>{item.name}</div>

    {#if item.stage === "error"}
      <div class="status error-text">{item.error ?? "Conversion failed"}</div>
    {:else if item.stage === "done"}
      <div class="status">
        {formatBytes(item.originalBytes)} → {formatBytes(item.outputBytes)}
        {#if savings(item)}
          <span class="savings">{savings(item)}</span>
        {/if}
      </div>
      {#if item.outputPath}
        <button class="reveal-btn" onclick={saveImage}>
          Save
        </button>
      {/if}
    {:else}
      <div class="status">
        {#if item.stage === "queued"}
          <span class="dots" aria-label="waiting">
            <span></span><span></span><span></span>
          </span>
          <span>Waiting…</span>
        {:else}
          {stageLabel[item.stage]}…
        {/if}
      </div>
    {/if}

    <div class="track">
      <div
        class="fill"
        class:indeterminate={item.stage === "queued"}
        style:width={item.stage === "queued" ? "100%" : `${item.percent}%`}
      ></div>
    </div>
  </div>
</div>

<style>
  .card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border-radius: var(--radius-lg);
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    transition:
      border-color var(--duration-fade-out) ease-out,
      background-color var(--duration-regular) var(--ease-standard);
    animation: pop-settle var(--duration-regular) var(--ease-out-quart) backwards;
  }

  .card.done {
    border-color: color-mix(in srgb, var(--success) 35%, var(--border-primary));
    animation:
      pop-settle var(--duration-regular) var(--ease-out-quart) backwards,
      success-flash 1.1s var(--ease-standard);
  }

  .card.error {
    border-color: color-mix(in srgb, var(--danger) 40%, var(--border-primary));
  }

  @keyframes pop-settle {
    0% {
      opacity: 0;
      transform: scale(0.94) translateY(6px);
    }
    100% {
      opacity: 1;
      transform: scale(1) translateY(0);
    }
  }

  @keyframes success-flash {
    0% {
      border-color: var(--success);
    }
    100% {
      border-color: color-mix(in srgb, var(--success) 35%, var(--border-primary));
    }
  }

  .thumb {
    position: relative;
    flex: none;
    width: 52px;
    height: 52px;
    border-radius: var(--radius-md);
    background: var(--bg-tertiary);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .thumb.scanning {
    background: var(--bg-tertiary);
  }

  .shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      100deg,
      transparent 30%,
      var(--accent-tint) 45%,
      var(--accent) 50%,
      var(--accent-tint) 55%,
      transparent 70%
    );
    background-size: 250% 100%;
    animation: shimmer-scan 1.6s var(--ease-in-out-cubic) infinite;
    opacity: 0.55;
  }

  @keyframes shimmer-scan {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -50% 0;
    }
  }

  .ext {
    position: relative;
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    letter-spacing: 0.02em;
    color: var(--text-tertiary);
    text-transform: uppercase;
  }

  .checkmark,
  .error-mark {
    width: 24px;
    height: 24px;
    animation: pop-in var(--duration-regular) var(--ease-out-quart) backwards;
  }

  @keyframes pop-in {
    0% {
      opacity: 0;
      transform: scale(0.6);
    }
    100% {
      opacity: 1;
      transform: scale(1);
    }
  }

  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .name {
    font-size: var(--text-small);
    font-weight: var(--weight-medium);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-mini);
    color: var(--text-tertiary);
  }

  .status.error-text {
    color: var(--danger);
  }

  .savings {
    color: var(--success);
    font-weight: var(--weight-semibold);
  }

  .track {
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--bg-tertiary);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent-tint);
    background-image: linear-gradient(90deg, var(--accent-tint), var(--accent));
    transition: width var(--duration-regular) var(--ease-standard);
  }

  .fill.indeterminate {
    width: 40% !important;
    background: var(--accent-tint);
    animation: indeterminate-sweep 1.3s var(--ease-in-out-cubic) infinite;
  }

  @keyframes indeterminate-sweep {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(250%);
    }
  }

  .dots {
    display: inline-flex;
    gap: 3px;
  }

  .dots span {
    width: 4px;
    height: 4px;
    border-radius: var(--radius-circle);
    background: var(--text-tertiary);
    animation: breathe 1.1s var(--ease-in-out-cubic) infinite;
  }

  .dots span:nth-child(2) {
    animation-delay: 0.12s;
  }
  .dots span:nth-child(3) {
    animation-delay: 0.24s;
  }

  @keyframes breathe {
    0%,
    100% {
      transform: translateY(0) scaleY(1);
      opacity: 0.5;
    }
    50% {
      transform: translateY(-2px) scaleY(1.15);
      opacity: 1;
    }
  }

  .reveal-btn {
    align-self: flex-start;
    background: transparent;
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-sm);
    color: var(--text-tertiary);
    cursor: pointer;
    font-family: inherit;
    font-size: var(--text-micro);
    font-weight: var(--weight-medium);
    padding: 3px 7px;
    transition:
      border-color var(--duration-quick) ease-in-out,
      color var(--duration-quick) ease-in-out,
      background-color var(--duration-quick) ease-in-out;
  }

  .reveal-btn:hover {
    background: var(--bg-tertiary);
    border-color: var(--border-secondary);
    color: var(--text-primary);
  }
</style>
