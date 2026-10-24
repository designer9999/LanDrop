<!--
  Peer avatar — colored shape with optional icon or first letter.
  Selected avatars morph from a circle to an expressive squircle.
  Sizes: sm (24px), md (32px), lg (40px), xl (48px)
-->
<script lang="ts">
  import { PEER_COLORS } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";

  interface Props {
    name: string;
    color: number;
    icon?: string;
    size?: "sm" | "md" | "lg" | "xl";
    selected?: boolean;
    /** Show a presence dot (true online, false offline, undefined hidden). */
    online?: boolean;
  }

  let { name, color, icon, size = "md", selected = false, online }: Props = $props();

  const initial = $derived([...name.trim()][0]?.toUpperCase() ?? "?");
  const bg = $derived(PEER_COLORS[color % PEER_COLORS.length]);
  const px = $derived({ sm: 24, md: 32, lg: 40, xl: 48 }[size]);
  const fs = $derived({ sm: 11, md: 14, lg: 17, xl: 20 }[size]);
  const iconSize = $derived({ sm: 15, md: 19, lg: 22, xl: 26 }[size]);
</script>

<span class="avatar-wrap" style:width="{px}px" style:height="{px}px">
  <span
    class="avatar"
    class:avatar-selected={selected}
    style:font-size="{fs}px"
    style:background-color={bg}
  >
    {#if icon}
      <Icon name={icon} size={iconSize} />
    {:else}
      {initial}
    {/if}
  </span>
  {#if online !== undefined}
    <span class="presence" class:presence-online={online} aria-hidden="true"></span>
  {/if}
</span>

<style>
  .avatar-wrap {
    position: relative;
    display: inline-flex;
    flex-shrink: 0;
  }
  .avatar {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    color: white;
    font-weight: 600;
    user-select: none;
    transition: border-radius var(--md-spring-default-spatial-dur) var(--md-spring-default-spatial);
  }
  .avatar-selected {
    border-radius: 32%;
  }
  .presence {
    position: absolute;
    right: -1px;
    bottom: -1px;
    width: 30%;
    height: 30%;
    min-width: 8px;
    min-height: 8px;
    border-radius: 50%;
    border: 2px solid var(--avatar-ring, var(--md-sys-color-surface));
    background: var(--md-sys-color-outline);
  }
  .presence-online {
    background: #2e9e5b;
  }
</style>
