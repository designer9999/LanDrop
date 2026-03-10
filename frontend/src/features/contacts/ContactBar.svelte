<!--
  Contact bar — horizontal scrollable chip row + Add button.
  When chips overflow, scroll arrows appear at the edges.
-->
<script lang="ts">
  import { getAppState } from "$lib/state/app-state.svelte";
  import IconButton from "$lib/ui/IconButton.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import ContactChip from "./ContactChip.svelte";

  interface Props {
    onadd: () => void;
    onedit: (id: string) => void;
  }

  let { onadd, onedit }: Props = $props();

  const app = getAppState();

  let scrollEl: HTMLDivElement | undefined = $state();
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function checkScroll() {
    if (!scrollEl) return;
    canScrollLeft = scrollEl.scrollLeft > 2;
    canScrollRight = scrollEl.scrollLeft < scrollEl.scrollWidth - scrollEl.clientWidth - 2;
  }

  function scrollBy(dir: number) {
    scrollEl?.scrollBy({ left: dir * 120, behavior: "smooth" });
  }

  $effect(() => {
    app.contacts.length;
    requestAnimationFrame(checkScroll);
  });
</script>

<div class="contact-bar-outer">
  {#if canScrollLeft}
    <button class="scroll-arrow scroll-arrow-left" onclick={() => scrollBy(-1)}>
      <Icon name="chevron_left" size={18} />
    </button>
  {/if}

  <div
    bind:this={scrollEl}
    class="contact-bar"
    onscroll={checkScroll}
  >
    {#each app.contacts as contact (contact.id)}
      <ContactChip
        {contact}
        selected={contact.id === app.activeContactId}
        onclick={() => {
          if (contact.id === app.activeContactId) {
            onedit(contact.id);
          } else {
            app.setActiveContact(contact.id);
          }
        }}
      />
    {/each}

    <IconButton title="Add contact" onclick={onadd}>
      <Icon name="person_add" size={20} />
    </IconButton>
  </div>

  {#if canScrollRight}
    <button class="scroll-arrow scroll-arrow-right" onclick={() => scrollBy(1)}>
      <Icon name="chevron_right" size={18} />
    </button>
  {/if}
</div>

<style>
  .contact-bar-outer {
    position: relative;
    width: 100%;
    overflow: hidden;
  }
  .contact-bar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 2px;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    scroll-behavior: smooth;
  }
  .contact-bar::-webkit-scrollbar {
    display: none;
  }
  .scroll-arrow {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 32px;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    cursor: pointer;
    color: var(--md-sys-color-on-surface);
    animation: fade-in var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects) both;
  }
  .scroll-arrow-left {
    left: 0;
    background: linear-gradient(to right, var(--md-sys-color-surface) 60%, transparent);
    padding-right: 8px;
  }
  .scroll-arrow-right {
    right: 0;
    background: linear-gradient(to left, var(--md-sys-color-surface) 60%, transparent);
    padding-left: 8px;
  }
  @keyframes fade-in {
    from { opacity: 0; }
    to   { opacity: 1; }
  }
</style>
