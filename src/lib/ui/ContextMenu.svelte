<!--
  M3 menu surface (surfaceContainer, 4dp corners → expressive 16dp, level-2 elevation).
  Rendered once; driven by `menu` in menu-state. Arrow keys move, Enter selects,
  Escape / outside click / scroll / resize dismiss.
-->
<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { menu, type MenuEntry, type MenuItem } from "./menu-state.svelte";

  let surface: HTMLDivElement | undefined = $state();
  let left = $state(0);
  let top = $state(0);

  const request = $derived(menu.current);

  function isItem(entry: MenuEntry): entry is MenuItem {
    return typeof entry === "object" && "label" in entry;
  }

  $effect(() => {
    if (!request) return;
    // Measure after render, then keep the surface fully inside the window.
    void tick().then(() => {
      if (!surface || !request) return;
      const { width, height } = surface.getBoundingClientRect();
      const margin = 8;
      left = Math.min(
        Math.max(margin, request.x - (request.returnFocus ? width : 0)),
        window.innerWidth - width - margin,
      );
      top =
        request.y + height + margin > window.innerHeight
          ? Math.max(margin, request.y - height - (request.returnFocus ? 48 : 0))
          : request.y;
      surface.querySelector<HTMLElement>("[data-menu-item]:not([disabled])")?.focus();
    });
  });

  function select(item: MenuItem) {
    if (item.disabled) return;
    menu.close();
    item.onselect();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (!request || !surface) return;
    if (event.key === "Escape") {
      event.preventDefault();
      menu.close();
      return;
    }
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    )
      return;
    event.preventDefault();
    const items = [...surface.querySelectorAll<HTMLElement>("[data-menu-item]:not([disabled])")];
    if (items.length === 0) return;
    const index = items.indexOf(document.activeElement as HTMLElement);
    let next = 0;
    if (event.key === "ArrowDown") next = (index + 1) % items.length;
    else if (event.key === "ArrowUp") next = (index - 1 + items.length) % items.length;
    else if (event.key === "End") next = items.length - 1;
    items[next].focus();
  }

  function handlePointerDown(event: PointerEvent) {
    if (request && surface && !surface.contains(event.target as Node)) menu.close();
  }
</script>

<svelte:window
  onkeydown={handleKeydown}
  onpointerdowncapture={handlePointerDown}
  onresize={() => request && menu.close()}
  onblur={() => request && menu.close()}
/>

{#if request}
  <div
    bind:this={surface}
    class="menu"
    role="menu"
    tabindex="-1"
    style:left="{left}px"
    style:top="{top}px"
    oncontextmenu={(event) => event.preventDefault()}
  >
    {#each request.entries as entry, index (index)}
      {#if entry === "divider"}
        <div class="menu-divider" role="separator"></div>
      {:else if isItem(entry)}
        <button
          class="menu-item"
          class:menu-item-danger={entry.danger}
          role="menuitem"
          data-menu-item
          disabled={entry.disabled}
          onclick={() => select(entry)}
        >
          {#if entry.icon}<Icon name={entry.icon} size={20} />{/if}
          <span class="menu-label">{entry.label}</span>
          {#if entry.hint}<span class="menu-hint">{entry.hint}</span>{/if}
        </button>
      {:else}
        <div class="menu-reactions" role="group" aria-label="Reactions">
          {#each entry.reactions as emoji (emoji)}
            <button
              class="menu-reaction"
              class:menu-reaction-selected={entry.selected === emoji}
              data-menu-item
              aria-label="React {emoji}"
              aria-pressed={entry.selected === emoji}
              onclick={() => {
                menu.close();
                entry.onreact(emoji);
              }}>{emoji}</button
            >
          {/each}
        </div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 1000;
    min-width: 200px;
    max-width: 300px;
    padding: 6px 0;
    border-radius: 16px;
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface);
    box-shadow: var(--shadow-level2);
    outline: none;
    transform-origin: top left;
    animation: menu-in var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  @keyframes menu-in {
    from {
      opacity: 0;
      transform: scale(0.92) translateY(-4px);
    }
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 44px;
    padding: 0 16px 0 14px;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 14px;
    text-align: left;
    cursor: pointer;
    color: var(--md-sys-color-on-surface);
  }
  .menu-item :global(.material-symbols-rounded) {
    color: var(--md-sys-color-on-surface-variant);
  }
  .menu-item:hover,
  .menu-item:focus-visible {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    outline: none;
  }
  .menu-item:disabled {
    opacity: 0.38;
    cursor: default;
  }
  .menu-item-danger,
  .menu-item-danger :global(.material-symbols-rounded) {
    color: var(--md-sys-color-error);
  }
  .menu-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .menu-hint {
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .menu-divider {
    height: 1px;
    margin: 6px 0;
    background: var(--md-sys-color-outline-variant);
  }
  .menu-reactions {
    display: flex;
    gap: 2px;
    padding: 2px 8px 6px;
  }
  .menu-reaction {
    flex: 1;
    min-width: 36px;
    height: 40px;
    border: none;
    border-radius: 20px;
    background: transparent;
    font-size: 20px;
    cursor: pointer;
    transition:
      transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      background-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .menu-reaction:hover,
  .menu-reaction:focus-visible {
    background: color-mix(in srgb, var(--md-sys-color-on-surface) 8%, transparent);
    transform: scale(1.18);
    outline: none;
  }
  .menu-reaction-selected {
    background: var(--md-sys-color-secondary-container);
  }
</style>
