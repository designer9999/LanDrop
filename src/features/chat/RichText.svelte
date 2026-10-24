<!--
  Message text: links, inline code and highlighted code blocks with Copy.
  Tokens become elements — message text is never parsed as HTML.
-->
<script lang="ts">
  import Icon from "$lib/ui/Icon.svelte";
  import { tokenizeMessage } from "$lib/chat/rich-text";
  import { copyToClipboard, openChatLink } from "$lib/api/bridge";

  interface Props {
    text: string;
    onsnackbar?: (msg: string) => void;
  }

  let { text, onsnackbar }: Props = $props();

  const tokens = $derived(tokenizeMessage(text));
  let copiedBlock = $state<number | null>(null);

  /** Highlight lazily: highlight.js loads only when a message contains a code block. */
  function highlight(lang: string) {
    return (node: HTMLElement) => {
      let cancelled = false;
      void import("highlight.js/lib/common").then(({ default: hljs }) => {
        if (cancelled) return;
        node.removeAttribute("data-highlighted");
        if (lang && hljs.getLanguage(lang)) node.classList.add(`language-${lang}`);
        hljs.highlightElement(node);
      });
      return () => {
        cancelled = true;
      };
    };
  }

  async function copyBlock(index: number, value: string) {
    await copyToClipboard(value);
    copiedBlock = index;
    setTimeout(() => {
      if (copiedBlock === index) copiedBlock = null;
    }, 1400);
  }

  async function open(event: MouseEvent, href: string) {
    event.preventDefault();
    event.stopPropagation();
    try {
      await openChatLink(href);
    } catch (error) {
      onsnackbar?.(`Couldn't open link: ${error instanceof Error ? error.message : error}`);
    }
  }
</script>

<div class="rich">
  {#each tokens as token, index (index)}
    {#if token.type === "text"}{token.value}{:else if token.type === "link"}<a
        href={token.href}
        title={token.href}
        onclick={(event) => open(event, token.href)}>{token.value}</a
      >{:else if token.type === "code"}<code class="inline-code">{token.value}</code>{:else}
      <div class="code-block">
        <div class="code-head">
          <span>{token.lang || "code"}</span>
          <button
            class="code-copy"
            onclick={(event) => {
              event.stopPropagation();
              void copyBlock(index, token.value);
            }}
            aria-label="Copy code"
          >
            <Icon name={copiedBlock === index ? "check" : "content_copy"} size={16} />
            {copiedBlock === index ? "Copied" : "Copy"}
          </button>
        </div>
        <pre><code {@attach highlight(token.lang)}>{token.value}</code></pre>
      </div>
    {/if}
  {/each}
</div>

<style>
  .rich {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  a {
    color: inherit;
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 2px;
    cursor: pointer;
  }
  a:hover {
    text-decoration-thickness: 2px;
  }
  .inline-code {
    font-family: var(--font-mono);
    font-size: 0.88em;
    padding: 1px 5px;
    border-radius: 6px;
    background: color-mix(in srgb, currentColor 12%, transparent);
  }
  .code-block {
    margin: 6px 0;
    border-radius: 12px;
    overflow: hidden;
    background: var(--md-sys-color-surface-container-lowest);
    color: var(--md-sys-color-on-surface);
    white-space: normal;
  }
  .code-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 6px 4px 12px;
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
    background: var(--md-sys-color-surface-container-high);
  }
  .code-copy {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 28px;
    padding: 0 10px;
    border: none;
    border-radius: 14px;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .code-copy:hover {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
  }
  pre {
    margin: 0;
    padding: 10px 12px;
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre;
  }
  pre :global(.hljs-keyword),
  pre :global(.hljs-built_in),
  pre :global(.hljs-selector-tag) {
    color: var(--md-sys-color-primary);
  }
  pre :global(.hljs-string),
  pre :global(.hljs-attr),
  pre :global(.hljs-template-tag) {
    color: var(--md-sys-color-tertiary);
  }
  pre :global(.hljs-number),
  pre :global(.hljs-literal),
  pre :global(.hljs-type) {
    color: var(--md-sys-color-secondary);
  }
  pre :global(.hljs-comment) {
    color: var(--md-sys-color-on-surface-variant);
    font-style: italic;
  }
  pre :global(.hljs-title),
  pre :global(.hljs-function) {
    color: var(--md-sys-color-on-surface);
    font-weight: 600;
  }
</style>
