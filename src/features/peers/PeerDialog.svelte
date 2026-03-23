<!--
  Add/Edit peer dialog — name, code phrase, color picker, save folder
-->
<script lang="ts">
  import Dialog from "$lib/ui/Dialog.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import { getAppState, PEER_COLORS, type Peer } from "$lib/state/app-state.svelte";
  import { pickSaveFolder } from "$lib/api/bridge";

  interface Props {
    open: boolean;
    editPeer?: Peer | null;
    onclose: () => void;
  }

  let { open = $bindable(false), editPeer = null, onclose }: Props = $props();

  const app = getAppState();

  let name = $state("");
  let code = $state("");
  let color = $state(0);
  let outFolder = $state("");
  let showDelete = $state(false);

  $effect(() => {
    if (open) {
      if (editPeer) {
        name = editPeer.name;
        code = editPeer.code;
        color = editPeer.color;
        outFolder = editPeer.outFolder ?? "";
      } else {
        name = "";
        code = "";
        color = Math.floor(Math.random() * PEER_COLORS.length);
        outFolder = "";
      }
      showDelete = false;
    }
  });

  const codeAsciiOnly = $derived(/^[\x20-\x7E]*$/.test(code.trim()));
  const isValid = $derived(name.trim().length > 0 && code.trim().length >= 4 && codeAsciiOnly);

  function handleSave() {
    if (!isValid) return;

    if (editPeer) {
      app.updatePeer(editPeer.id, {
        name: name.trim(),
        code: code.trim(),
        color,
        outFolder: outFolder || undefined,
      });
    } else {
      const peer: Peer = {
        id: crypto.randomUUID(),
        name: name.trim(),
        code: code.trim(),
        color,
        lastUsedAt: new Date().toISOString(),
        outFolder: outFolder || undefined,
      };
      app.addPeer(peer);
      app.setActivePeer(peer.id);
    }

    open = false;
    onclose();
  }

  function handleDelete() {
    if (editPeer) {
      app.removePeer(editPeer.id);
    }
    open = false;
    onclose();
  }
</script>

<Dialog
  bind:open
  headline={editPeer ? "Edit Peer" : "Add Peer"}
  confirmLabel="Save"
  confirmDisabled={!isValid}
  onconfirm={handleSave}
  onclose={onclose}
>
  <div class="flex flex-col gap-4">
    <TextField
      label="Name"
      placeholder="e.g. Office PC"
      bind:value={name}
    />

    <TextField
      label="Code phrase"
      placeholder="Shared password (min 4 chars)"
      mono
      bind:value={code}
    />

    {#if code.trim().length > 0 && !codeAsciiOnly}
      <div class="text-xs text-error">
        Code must contain only English letters, numbers, and symbols
      </div>
    {:else}
      <div class="text-xs text-on-surface-variant">
        Both devices must use the same code phrase.
        On the same network, transfers are instant via LAN direct.
      </div>
    {/if}

    <!-- Save folder -->
    <div>
      <div class="text-xs text-on-surface-variant mb-2">Save folder (optional)</div>
      {#if outFolder}
        <div class="flex items-center gap-2 h-10 px-3 bg-surface-container-lowest border border-outline-variant rounded-xs">
          <span class="text-tertiary"><Icon name="folder" size={16} /></span>
          <span class="flex-1 text-xs text-on-surface font-mono truncate">{outFolder}</span>
          <button
            class="w-7 h-7 inline-flex items-center justify-center rounded-full
                   text-on-surface-variant hover:text-error cursor-pointer bg-transparent border-none"
            onclick={() => outFolder = ""}
          >
            <Icon name="close" size={16} />
          </button>
        </div>
      {:else}
        <Button variant="outlined" full onclick={async () => {
          const f = await pickSaveFolder();
          if (f) outFolder = f;
        }}>
          <Icon name="create_new_folder" size={18} />
          Choose folder
        </Button>
      {/if}
    </div>

    <!-- Color picker -->
    <div>
      <div class="text-xs text-on-surface-variant mb-2">Color</div>
      <div class="flex gap-2">
        {#each PEER_COLORS as bg, i}
          <button
            class="w-7 h-7 rounded-full cursor-pointer border-2 shrink-0"
            aria-label="Color {i + 1}"
            style="
              background-color: {bg};
              border-color: {i === color ? 'var(--md-sys-color-on-surface)' : 'transparent'};
              transition: border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
            "
            onclick={() => color = i}
          ></button>
        {/each}
      </div>
    </div>

    <!-- Delete button (edit mode only) -->
    {#if editPeer}
      <div class="pt-2 border-t border-outline-variant">
        {#if !showDelete}
          <button
            class="text-sm text-error cursor-pointer bg-transparent border-none px-0"
            onclick={() => showDelete = true}
          >
            Delete this peer
          </button>
        {:else}
          <div class="flex items-center gap-2">
            <span class="text-sm text-error">Are you sure?</span>
            <Button variant="error" onclick={handleDelete}>
              Delete
            </Button>
            <Button variant="outlined" onclick={() => showDelete = false}>
              No
            </Button>
          </div>
        {/if}
      </div>
    {/if}
  </div>
</Dialog>
