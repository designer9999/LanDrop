<!--
  Session activity log — collapsible, shows transfer history.
  Received file entries are clickable — opens the output folder.
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { openFolder } from "$lib/api/bridge";
  import type { ActivityEntry } from "$lib/state/app-state.svelte";

  const app = getAppState();

  let open = $state(false);

  const entries = $derived(
    app.activeContact
      ? app.activity.filter(a => a.contactId === app.activeContact!.id)
      : app.activity
  );

  function formatTime(iso: string): string {
    return new Date(iso).toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", hour12: false });
  }

  async function handleClick(entry: ActivityEntry) {
    if (entry.direction !== "received" || entry.type === "text") return;
    const folder = entry.outFolder || app.receiveOptions.outFolder;
    if (folder) {
      await openFolder(folder);
    }
  }

  function isClickable(entry: ActivityEntry): boolean {
    if (entry.direction !== "received" || entry.type === "text") return false;
    return !!(entry.outFolder || app.receiveOptions.outFolder);
  }
</script>

{#if entries.length > 0}
  <Card variant="outlined">
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="flex items-center gap-2 text-on-surface text-sm font-medium cursor-pointer"
      onclick={() => open = !open}
    >
      <span class="text-primary"><Icon name="history" size={20} /></span>
      Activity
      <span class="text-xs font-normal text-on-surface-variant">({entries.length})</span>
      <span class="flex-1"></span>
      {#if open}
        <button
          class="text-xs text-on-surface-variant cursor-pointer bg-transparent border-none px-1 py-0.5
                 hover:text-error"
          style="transition: color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
          onclick={(e) => { e.stopPropagation(); app.clearActivity(); }}
        >
          Clear
        </button>
      {/if}
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={open}>
        <Icon name="expand_more" size={20} />
      </span>
    </div>

    {#if open}
      <div class="flex flex-col gap-1 mt-3 max-h-48 overflow-y-auto section-enter">
        {#each entries.toReversed() as entry (entry.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="flex items-center gap-2 py-1.5 px-2 rounded-lg text-sm
                   {isClickable(entry) ? 'cursor-pointer hover:bg-surface-container-high' : ''}"
            style="transition: background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
            onclick={() => handleClick(entry)}
          >
            <span class={entry.direction === "sent" ? "text-primary" : "text-tertiary"}>
              <Icon name={entry.direction === "sent" ? "upload" : "download"} size={16} />
            </span>
            <div class="flex-1 min-w-0">
              <div class="truncate {entry.success ? 'text-on-surface' : 'text-error'}">
                {#if entry.type === "text"}
                  Text message
                {:else if entry.items.length > 0}
                  {entry.items.join(", ")}
                {:else}
                  {entry.direction === "sent" ? "Files sent" : "Files received"}
                {/if}
              </div>
              {#if !entry.success}
                <div class="text-xs text-error">Failed</div>
              {/if}
            </div>
            <span class="text-xs text-outline shrink-0">{formatTime(entry.timestamp)}</span>
            {#if isClickable(entry)}
              <button
                class="text-on-surface-variant bg-transparent border-none cursor-pointer p-0.5 rounded-full
                       hover:text-primary"
                style="transition: color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
                title="Open folder"
                onclick={(e) => { e.stopPropagation(); handleClick(entry); }}
              >
                <Icon name="folder_open" size={16} />
              </button>
            {:else}
              <span class={entry.success ? "text-tertiary" : "text-error"}>
                <Icon name={entry.success ? "check_circle" : "error"} size={14} />
              </span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </Card>
{/if}

<style>
  .section-arrow {
    transition: transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .section-arrow-open {
    transform: rotate(180deg);
  }
  .section-enter {
    animation: section-slide var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial) both;
  }
  @keyframes section-slide {
    from { opacity: 0; transform: translateY(-8px); }
    to   { opacity: 1; transform: translateY(0); }
  }
</style>
