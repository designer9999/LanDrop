<!--
  Main transfer view — per-contact or fallback mode
  Assembles: UnifiedSendArea + QuickReceive + CodeDisplay + ActivityLog + LogPanel
-->
<script lang="ts">
  import { getAppState } from "$lib/state/app-state.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Card from "$lib/ui/Card.svelte";
  import Button from "$lib/ui/Button.svelte";
  import UnifiedSendArea from "./UnifiedSendArea.svelte";
  import QuickReceive from "./QuickReceive.svelte";
  import ActivityLog from "./ActivityLog.svelte";
  import CodeDisplay from "../send/CodeDisplay.svelte";
  import LogPanel from "../LogPanel.svelte";

  interface Props {
    onsnackbar?: (msg: string) => void;
    onaddcontact?: () => void;
  }

  let { onsnackbar, onaddcontact }: Props = $props();

  const app = getAppState();
  const contact = $derived(app.activeContact);
</script>

<div class="flex flex-col gap-4">

  {#if !app.contacts.length}
    <!-- No contacts — onboarding -->
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
  {/if}

  <!-- Unified send -->
  <UnifiedSendArea contactName={contact?.name} {onsnackbar} />

  <!-- Transfer code display (only for manual/no-contact sends) -->
  {#if app.transferCode && !contact}
    <CodeDisplay code={app.transferCode} oncopied={() => onsnackbar?.("Code copied!")} />
  {/if}

  <!-- Quick receive -->
  <QuickReceive contactName={contact?.name} {onsnackbar} />

  <!-- Activity log -->
  <ActivityLog />

  <!-- Raw croc log (collapsible) -->
  <LogPanel />

</div>
