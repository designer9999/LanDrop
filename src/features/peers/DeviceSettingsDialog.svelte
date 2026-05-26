<!--
  Device settings dialog — view device info, change color, set per-device save folder, forget device
-->
<script lang="ts">
  import Dialog from "$lib/ui/Dialog.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import PeerAvatar from "./PeerAvatar.svelte";
  import { getAppState, PEER_COLORS, type DiscoveredDevice } from "$lib/state/app-state.svelte";
  import { pickSaveFolder } from "$lib/api/bridge";

  interface Props {
    open: boolean;
    device: DiscoveredDevice | null;
    onclose: () => void;
  }

  let { open = $bindable(false), device, onclose }: Props = $props();

  const app = getAppState();

  const AVATAR_ICONS = [
    "person",
    "computer",
    "phone_android",
    "home",
    "work",
    "school",
    "sports_esports",
    "headphones",
    "photo_camera",
    "movie",
    "brush",
    "code",
  ];

  function nextAvatarIcon(current: string | undefined): string | undefined {
    if (!current) return AVATAR_ICONS[0];
    const nextIndex = AVATAR_ICONS.indexOf(current) + 1;
    return AVATAR_ICONS[nextIndex] ?? undefined;
  }

  let color = $state(0);
  let avatarIcon = $state<string | undefined>(undefined);
  let outFolder = $state("");
  let showDelete = $state(false);

  $effect(() => {
    if (open && device) {
      color = device.color;
      avatarIcon = device.avatarIcon;
      outFolder = device.outFolder ?? "";
      showDelete = false;
    }
  });

  function handleSave() {
    if (!device) return;
    app.updateDeviceSettings(device.id, {
      color,
      avatarIcon,
      outFolder: outFolder || undefined,
    });
    open = false;
    onclose();
  }

  function handleForget() {
    if (!device) return;
    app.removeDevice(device.id);
    open = false;
    onclose();
  }
</script>

<Dialog
  bind:open
  headline="Device Settings"
  confirmLabel="Save"
  onconfirm={handleSave}
  onclose={onclose}
>
  {#if device}
    <div class="flex flex-col gap-4">

      <!-- Device info -->
      <div class="device-info">
        <button class="avatar-edit" onclick={() => avatarIcon = nextAvatarIcon(avatarIcon)} title="Change avatar icon">
          <PeerAvatar name={device.alias} {color} icon={avatarIcon} size="lg" />
        </button>
        <div class="device-details">
          <div class="device-name">{device.alias}</div>
          <div class="device-meta">
            <span class="device-type">
              <Icon name={device.deviceType === "mobile" ? "phone_android" : "computer"} size={14} />
              {device.deviceType === "mobile" ? "Mobile" : "Desktop"}
            </span>
            <span class="device-ip">{device.ip}</span>
          </div>
          <div class="device-status" class:online={device.online}>
            <span class="status-indicator"></span>
            {device.online ? "Online" : "Offline"}
          </div>
        </div>
      </div>

      <!-- Avatar icon picker -->
      <div>
        <div class="text-xs text-on-surface-variant mb-2">Avatar icon</div>
        <div class="avatar-grid">
          <button
            class="avatar-option"
            class:avatar-option-active={!avatarIcon}
            aria-label="Use first letter"
            title="First letter"
            onclick={() => avatarIcon = undefined}
          >
            <span class="avatar-letter">D</span>
          </button>
          {#each AVATAR_ICONS as iconName}
            <button
              class="avatar-option"
              class:avatar-option-active={avatarIcon === iconName}
              aria-label="Use {iconName} icon"
              title={iconName.replaceAll("_", " ")}
              onclick={() => avatarIcon = iconName}
            >
              <Icon name={iconName} size={19} />
            </button>
          {/each}
        </div>
      </div>

      <!-- Save folder -->
      <div>
        <div class="text-xs text-on-surface-variant mb-2">Save folder for this device (optional)</div>
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

      <!-- Forget device -->
      <div class="pt-2 border-t border-outline-variant">
        {#if !showDelete}
          <button
            class="text-sm text-error cursor-pointer bg-transparent border-none px-0"
            onclick={() => showDelete = true}
          >
            Forget this device
          </button>
        {:else}
          <div class="flex items-center gap-2">
            <span class="text-sm text-error">Remove from device list?</span>
            <Button variant="error" onclick={handleForget}>
              Remove
            </Button>
            <Button variant="outlined" onclick={() => showDelete = false}>
              No
            </Button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</Dialog>

<style>
  .device-info {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 16px;
    border-radius: 16px;
    background: var(--md-sys-color-surface-container);
  }
  .avatar-edit {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    cursor: pointer;
  }
  .avatar-edit:hover {
    filter: brightness(1.08);
  }
  .device-details {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .device-name {
    font-size: 16px;
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .device-meta {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--md-sys-color-on-surface-variant);
  }
  .device-type {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .device-ip {
    font-family: "Roboto Mono", monospace;
    font-size: 11px;
    opacity: 0.7;
  }
  .device-status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--md-sys-color-outline);
  }
  .device-status.online {
    color: var(--md-sys-color-tertiary);
  }
  .status-indicator {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--md-sys-color-outline);
  }
  .device-status.online .status-indicator {
    background: var(--md-sys-color-tertiary);
  }
  .avatar-grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 8px;
  }
  .avatar-option {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: 1px solid var(--md-sys-color-outline-variant);
    background: var(--md-sys-color-surface-container-low);
    color: var(--md-sys-color-on-surface-variant);
    cursor: pointer;
    transition:
      background var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      transform var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial);
  }
  .avatar-option:hover {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface);
  }
  .avatar-option:active {
    transform: scale(0.94);
  }
  .avatar-option-active {
    border-color: var(--md-sys-color-primary);
    background: var(--md-sys-color-primary-container);
    color: var(--md-sys-color-on-primary-container);
  }
  .avatar-letter {
    font-size: 15px;
    font-weight: 600;
  }
</style>
