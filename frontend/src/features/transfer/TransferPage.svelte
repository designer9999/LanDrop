<!--
  Main transfer view — Claude-style chat layout
  Chat fills the space, activity/log are collapsible below
-->
<script lang="ts">
  import { getAppState } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Card from "$lib/ui/Card.svelte";
  import Button from "$lib/ui/Button.svelte";
  import UnifiedSendArea from "./UnifiedSendArea.svelte";
  import ActivityLog from "./ActivityLog.svelte";
  import LogPanel from "../LogPanel.svelte";

  interface Props {
    onsnackbar?: (msg: string) => void;
    onaddcontact?: () => void;
    onsend?: () => void;
    onsendtext?: () => void;
  }

  let { onsnackbar, onaddcontact, onsend, onsendtext }: Props = $props();

  const app = getAppState();
  const contact = $derived(app.activeContact);
</script>

{#if !app.contacts.length}
  <!-- No contacts — onboarding -->
  <div class="p-4">
    <Card variant="filled">
      <div class="flex flex-col items-center text-center gap-3 py-4">
        <span class="text-primary"><Icon name="group_add" size={48} /></span>
        <div>
          <div class="text-lg font-medium text-on-surface">Welcome to Croc Transfer</div>
          <div class="text-sm text-on-surface-variant mt-1">
            Add a contact with a shared code phrase to start transferring files with one click.
          </div>
        </div>
        <Button onclick={onaddcontact}>
          <Icon name="person_add" size={18} />
          Add Contact
        </Button>
      </div>
    </Card>
  </div>
{/if}

<!-- Chat area — fills available space -->
<UnifiedSendArea contactName={contact?.name} {onsnackbar} {onsend} {onsendtext} />

<!-- LAN status indicator -->
{#if contact}
  <div class="lan-status">
    {#if app.lanConnected}
      <span class="text-primary flex items-center gap-1">
        <Icon name="bolt" size={12} />
        LAN direct — {app.lanPeerIp}
      </span>
    {:else}
      <span class="flex items-center gap-1 text-on-surface-variant">
        <Icon name="sync" size={12} />
        Searching LAN...
      </span>
    {/if}
  </div>
{/if}

<style>
  .lan-status {
    display: flex;
    justify-content: center;
    padding: 2px 8px;
    font-size: 10px;
    flex-shrink: 0;
  }
</style>
