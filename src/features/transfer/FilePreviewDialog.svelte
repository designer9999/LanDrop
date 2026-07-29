<!--
  File content preview modal — syntax highlighting, line numbers, copy.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { showInExplorer, copyToClipboard } from "$lib/api/bridge";
  import type { FilePreview } from "$lib/api/bridge";
  import hljs from "highlight.js/lib/common";

  interface Props {
    preview: FilePreview | null;
    path: string;
    loading: boolean;
    onclose: () => void;
    onsnackbar?: (msg: string) => void;
  }

  let { preview, path, loading, onclose, onsnackbar }: Props = $props();

  let copied = $state(false);
  let codeEl = $state<HTMLElement | null>(null);

  // Map common extensions to highlight.js language names
  const EXT_MAP: Record<string, string> = {
    js: "javascript", ts: "typescript", jsx: "javascript", tsx: "typescript",
    py: "python", rb: "ruby", rs: "rust", go: "go", java: "java",
    kt: "kotlin", swift: "swift", cs: "csharp", cpp: "cpp", c: "c", h: "c",
    php: "php", sh: "bash", bash: "bash", zsh: "bash", ps1: "shell",
    json: "json", yaml: "yaml", yml: "yaml", toml: "ini", xml: "xml",
    html: "xml", htm: "xml", css: "css", scss: "scss", less: "less",
    sql: "sql", md: "markdown", txt: "", csv: "", log: "",
    svelte: "xml", vue: "xml", dockerfile: "bash",
    makefile: "makefile", gitignore: "", env: "bash",
  };

  const ext = $derived((preview?.extension ?? "").replace(".", "").toLowerCase());
  const lang = $derived(EXT_MAP[ext] ?? "");

  // Highlight after the code element is rendered
  $effect(() => {
    if (codeEl && preview?.content) {
      // Reset and re-highlight
      codeEl.textContent = preview.content;
      delete (codeEl.dataset as any).highlighted;
      if (lang) {
        codeEl.className = `language-${lang}`;
      } else {
        codeEl.className = "";
      }
      hljs.highlightElement(codeEl);
    }
  });

  async function handleCopy() {
    if (!preview?.content) return;
    await copyToClipboard(preview.content);
    copied = true;
    onsnackbar?.("Copied to clipboard");
    setTimeout(() => { copied = false; }, 1500);
  }

  function getLines(content: string): number[] {
    return Array.from({ length: content.split("\n").length }, (_, i) => i + 1);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="file-preview-overlay" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="file-preview-modal" onclick={(e) => e.stopPropagation()}>
    <div class="file-preview-header">
      <div class="file-preview-title">
        <span class="file-preview-name">{preview?.name ?? '...'}</span>
        {#if preview}
          <span class="file-preview-meta">
            {preview.size}
            {#if preview.line_count} &bull; {preview.line_count} lines{/if}
            {#if lang} &bull; {lang}{/if}
            {#if preview.truncated} &bull; Truncated{/if}
          </span>
        {/if}
      </div>
      <div class="file-preview-actions">
        <button class="overlay-btn" onclick={handleCopy} title="Copy all">
          <Icon name={copied ? "check" : "content_copy"} size={18} />
        </button>
        <button class="overlay-btn" onclick={() => showInExplorer(path)} title="Show in folder">
          <Icon name="folder_open" size={18} />
        </button>
        <button class="overlay-btn" onclick={onclose}>
          <Icon name="close" size={20} />
        </button>
      </div>
    </div>
    <div class="file-preview-body">
      {#if loading}
        <div class="file-preview-loading">Loading preview...</div>
      {:else if preview?.content}
        <div class="code-container">
          <div class="line-numbers" aria-hidden="true">
            {#each getLines(preview.content) as num}
              <span>{num}</span>
            {/each}
          </div>
          <pre class="code-block"><code bind:this={codeEl}>{preview.content}</code></pre>
        </div>
      {:else}
        <div class="file-preview-loading">No preview available for this file type.</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .file-preview-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.7);
    animation: overlay-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) both;
    cursor: pointer;
  }
  @keyframes overlay-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  .file-preview-modal {
    display: flex;
    flex-direction: column;
    width: 90%;
    max-width: 700px;
    max-height: 80%;
    border-radius: 16px;
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface);
    box-shadow: 0 8px 32px rgba(0,0,0,0.5);
    cursor: default;
    overflow: hidden;
    animation: dialog-scale-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes dialog-scale-in {
    from { transform: scale(0.95); opacity: 0; }
    to   { transform: scale(1); opacity: 1; }
  }
  .file-preview-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 14px 16px 12px;
    border-bottom: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 20%, transparent);
    flex-shrink: 0;
    background: var(--md-sys-color-surface-container-high);
  }
  .file-preview-title {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }
  .file-preview-name {
    font-size: 14px;
    font-weight: 600;
    word-break: break-word;
    color: var(--md-sys-color-on-surface);
  }
  .file-preview-meta {
    font-size: 11px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .file-preview-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    margin-left: 12px;
  }
  .overlay-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 6px;
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    flex-shrink: 0;
    transition: background 0.15s;
  }
  .overlay-btn:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 15%, transparent);
    color: var(--md-sys-color-on-surface);
  }
  .file-preview-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-on-surface) 15%, transparent) transparent;
  }
  .code-container {
    display: flex;
    min-height: 100%;
  }
  .line-numbers {
    display: flex;
    flex-direction: column;
    padding: 16px 0;
    text-align: right;
    user-select: none;
    flex-shrink: 0;
    background: var(--md-sys-color-surface-container-high);
    border-right: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 15%, transparent);
  }
  .line-numbers span {
    display: block;
    padding: 0 12px 0 16px;
    font-family: "Roboto Mono", "Cascadia Code", "Fira Code", monospace;
    font-size: 12px;
    line-height: 1.6;
    color: var(--md-sys-color-outline);
    min-width: 32px;
  }
  .code-block {
    margin: 0;
    padding: 16px;
    font-family: "Roboto Mono", "Cascadia Code", "Fira Code", monospace;
    font-size: 12px;
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-word;
    flex: 1;
    user-select: text;
    cursor: text;
  }
  .file-preview-loading {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 40px 20px;
    font-size: 13px;
    color: var(--md-sys-color-on-surface-variant);
  }

  /* Syntax highlighting — M3 themed */
  :global(.hljs) { background: transparent; color: var(--md-sys-color-on-surface); }
  :global(.hljs-keyword),
  :global(.hljs-selector-tag),
  :global(.hljs-built_in),
  :global(.hljs-type) { color: var(--md-sys-color-primary); }
  :global(.hljs-string),
  :global(.hljs-attr),
  :global(.hljs-attribute) { color: var(--md-sys-color-tertiary); }
  :global(.hljs-tag),
  :global(.hljs-name),
  :global(.hljs-title),
  :global(.hljs-title.function_) { color: var(--md-sys-color-secondary); }
  :global(.hljs-number),
  :global(.hljs-literal),
  :global(.hljs-symbol),
  :global(.hljs-bullet) { color: var(--md-sys-color-primary); opacity: 0.8; }
  :global(.hljs-comment),
  :global(.hljs-quote),
  :global(.hljs-meta) { color: var(--md-sys-color-outline); font-style: italic; }
  :global(.hljs-punctuation),
  :global(.hljs-operator) { color: var(--md-sys-color-on-surface-variant); }
  :global(.hljs-variable),
  :global(.hljs-template-variable),
  :global(.hljs-params) { color: var(--md-sys-color-on-surface); }
  :global(.hljs-regexp),
  :global(.hljs-link) { color: var(--md-sys-color-error); }
</style>
