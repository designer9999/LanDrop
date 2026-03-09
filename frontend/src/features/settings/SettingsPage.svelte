<!--
  Settings page — send/receive defaults, about & status
-->
<script lang="ts">
  import Card from "$lib/ui/Card.svelte";
  import Icon from "$lib/ui/Icon.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import TextField from "$lib/ui/TextField.svelte";
  import Select from "$lib/ui/Select.svelte";
  import Button from "$lib/ui/Button.svelte";
  import { getAppState } from "$lib/state/app-state.svelte";
  import { pickSaveFolder, installCroc, openUrl, copyToClipboard, setNotifications } from "$lib/api/bridge";

  interface Props {
    appVersion: string;
    onsnackbar?: (msg: string) => void;
  }

  let { appVersion, onsnackbar }: Props = $props();

  const app = getAppState();

  let sendOpen = $state(true);
  let receiveOpen = $state(true);
  let aboutOpen = $state(false);

  const curveOptions = [
    { value: "",        label: "P-256 (default)" },
    { value: "p384",    label: "P-384" },
    { value: "p521",    label: "P-521 (strongest)" },
    { value: "siec",    label: "SIEC" },
    { value: "ed25519", label: "Ed25519" },
  ];

  const hashOptions = [
    { value: "",        label: "xxhash (default, fastest)" },
    { value: "imohash", label: "imohash (fast for large files)" },
    { value: "md5",     label: "MD5" },
  ];

  async function handleCopy(text: string, label: string) {
    await copyToClipboard(text);
    onsnackbar?.(`Copied: ${label}`);
  }
</script>

<div class="flex flex-col gap-4">

  <!-- Send defaults -->
  <Card variant="outlined">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => sendOpen = !sendOpen}
    >
      <span class="text-primary"><Icon name="upload" size={20} /></span>
      Send defaults
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={sendOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if sendOpen}
      <div class="flex flex-col gap-5 mt-4 section-enter" style="--tf-bg: var(--color-surface);">
        <p class="text-xs text-on-surface-variant -mb-2">
          These apply to all transfers. Per-contact overrides are in the contact editor.
        </p>

        <!-- Quick access folder -->
        <div>
          <p class="text-xs font-medium text-on-surface-variant mb-2">Quick access folder</p>
          {#if app.sendOptions.sourceFolder}
            <div class="flex items-center gap-2 h-10 px-3 bg-surface-container-lowest border border-outline-variant rounded-xs">
              <span class="text-tertiary"><Icon name="folder_open" size={16} /></span>
              <span class="flex-1 text-xs text-on-surface font-mono truncate">{app.sendOptions.sourceFolder}</span>
              <button
                class="w-7 h-7 inline-flex items-center justify-center rounded-full
                       text-on-surface-variant hover:text-error cursor-pointer bg-transparent border-none"
                onclick={() => app.updateSendOption("sourceFolder", undefined)}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          {:else}
            <Button variant="outlined" full onclick={async () => {
              const f = await pickSaveFolder();
              if (f) app.updateSendOption("sourceFolder", f);
            }}>
              <Icon name="create_new_folder" size={18} />
              Set quick access folder
            </Button>
          {/if}
        </div>

        <Select label="Encryption curve" options={curveOptions}
          value={app.sendOptions.curve ?? ""}
          onchange={(v) => app.updateSendOption("curve", v || undefined)} />

        <Select label="Hash algorithm" options={hashOptions}
          value={app.sendOptions.hash ?? ""}
          onchange={(v) => app.updateSendOption("hash", v || undefined)} />

        <TextField label="Exclude patterns" placeholder="node_modules,.git,.venv"
          supportingText="Comma-separated. Skips matching files/folders." mono
          value={app.sendOptions.exclude ?? ""}
          oninput={(e) => app.updateSendOption("exclude", (e.target as HTMLInputElement).value || undefined)} />

        <TextField label="Upload speed limit" placeholder="500k or 2M"
          supportingText="Leave empty for unlimited." mono
          value={app.sendOptions.throttleUpload ?? ""}
          oninput={(e) => app.updateSendOption("throttleUpload", (e.target as HTMLInputElement).value || undefined)} />

        <div class="flex flex-col gap-3">
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">Zip folders</div><div class="text-xs text-on-surface-variant">Compress folders to .zip before sending</div></div>
            <Switch checked={app.sendOptions.zip ?? false} onchange={(v) => app.updateSendOption("zip", v || undefined)} />
          </div>
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">Respect .gitignore</div><div class="text-xs text-on-surface-variant">Skip files in .gitignore</div></div>
            <Switch checked={app.sendOptions.git ?? false} onchange={(v) => app.updateSendOption("git", v || undefined)} />
          </div>
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">No compression</div><div class="text-xs text-on-surface-variant">Faster for pre-compressed files (zip, mp4)</div></div>
            <Switch checked={app.sendOptions.noCompress ?? false} onchange={(v) => app.updateSendOption("noCompress", v || undefined)} />
          </div>
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">Local only</div><div class="text-xs text-on-surface-variant">LAN transfer — no internet relay</div></div>
            <Switch checked={app.sendOptions.local ?? false} onchange={(v) => app.updateSendOption("local", v || undefined)} />
          </div>
        </div>
      </div>
    {/if}
  </Card>

  <!-- Receive defaults -->
  <Card variant="outlined">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0 -my-0.5"
      onclick={() => receiveOpen = !receiveOpen}
    >
      <span class="text-primary"><Icon name="download" size={20} /></span>
      Receive defaults
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={receiveOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if receiveOpen}
      <div class="flex flex-col gap-4 mt-4 section-enter" style="--tf-bg: var(--color-surface);">
        <!-- Save folder -->
        <div>
          <p class="text-xs font-medium text-on-surface-variant mb-2">Default save folder</p>
          {#if app.receiveOptions.outFolder}
            <div class="flex items-center gap-2 h-10 px-3 bg-surface-container-lowest border border-outline-variant rounded-xs">
              <span class="text-tertiary"><Icon name="folder" size={16} /></span>
              <span class="flex-1 text-xs text-on-surface font-mono truncate">{app.receiveOptions.outFolder}</span>
              <button
                class="w-7 h-7 inline-flex items-center justify-center rounded-full
                       text-on-surface-variant hover:text-error cursor-pointer bg-transparent border-none"
                onclick={() => app.updateReceiveOption("outFolder", undefined)}
              >
                <Icon name="close" size={16} />
              </button>
            </div>
          {:else}
            <Button variant="outlined" full onclick={async () => {
              const f = await pickSaveFolder();
              if (f) app.updateReceiveOption("outFolder", f);
            }}>
              <Icon name="create_new_folder" size={18} />
              Set save folder
            </Button>
          {/if}
        </div>

        <div class="flex flex-col gap-3">
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">Overwrite files</div><div class="text-xs text-on-surface-variant">Replace existing files without asking</div></div>
            <Switch checked={app.receiveOptions.overwrite ?? false} onchange={(v) => app.updateReceiveOption("overwrite", v || undefined)} />
          </div>
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">No compression</div><div class="text-xs text-on-surface-variant">Must match sender's setting</div></div>
            <Switch checked={app.receiveOptions.noCompress ?? false} onchange={(v) => app.updateReceiveOption("noCompress", v || undefined)} />
          </div>
          <div class="flex items-center justify-between py-1">
            <div><div class="text-sm text-on-surface">Local only</div><div class="text-xs text-on-surface-variant">LAN transfer — no internet relay</div></div>
            <Switch checked={app.receiveOptions.local ?? false} onchange={(v) => app.updateReceiveOption("local", v || undefined)} />
          </div>
        </div>
      </div>
    {/if}
  </Card>

  <!-- Notifications -->
  <Card variant="outlined">
    <div class="flex items-center gap-2 text-on-surface text-sm font-medium">
      <span class="text-primary"><Icon name="notifications" size={20} /></span>
      Notifications
    </div>
    <div class="flex items-center justify-between py-1 mt-3">
      <div>
        <div class="text-sm text-on-surface">Notification sounds</div>
        <div class="text-xs text-on-surface-variant">Play sound when files or messages arrive while app is unfocused</div>
      </div>
      <Switch checked={app.notificationsEnabled} onchange={(v) => { app.setNotifications(v); setNotifications(v); }} />
    </div>
  </Card>

  <!-- About & Status (last, less important) -->
  <Card variant="filled">
    <button
      class="w-full flex items-center gap-2 text-on-surface text-sm font-medium
             cursor-pointer border-none bg-transparent p-0"
      onclick={() => aboutOpen = !aboutOpen}
    >
      <span class="text-primary"><Icon name="info" size={20} /></span>
      About & Status
      <span class="flex-1"></span>
      <span class="text-on-surface-variant section-arrow" class:section-arrow-open={aboutOpen}>
        <Icon name="expand_more" size={20} />
      </span>
    </button>

    {#if aboutOpen}
      <div class="flex flex-col gap-3 mt-4 section-enter">
        <!-- croc status -->
        <div class="flex items-center gap-3 p-3 rounded-xl bg-surface-container">
          <div class="w-3 h-3 rounded-full shrink-0 {app.crocOk ? 'bg-tertiary' : 'bg-error animate-pulse'}"></div>
          <div class="flex-1">
            {#if app.crocOk}
              <div class="text-sm text-on-surface font-medium">croc v{app.crocVersion}</div>
              <div class="text-xs text-on-surface-variant">Ready to transfer</div>
            {:else}
              <div class="text-sm text-error font-medium">croc not installed</div>
            {/if}
          </div>
          {#if !app.crocOk}
            <Button onclick={async () => await installCroc()} disabled={app.crocInstalling}>
              {app.crocInstalling ? "Installing..." : "Install"}
            </Button>
          {/if}
        </div>

        <div class="text-xs text-on-surface-variant">App version: v{appVersion}</div>

        <!-- Quick links -->
        <div class="grid grid-cols-2 gap-2">
          <button
            class="flex items-center gap-2 p-2.5 rounded-xl bg-surface-container text-xs text-on-surface-variant
                   cursor-pointer border-none hover:bg-surface-container-high transition-colors"
            onclick={() => handleCopy("winget install schollz.croc", "install command")}
          >
            <span class="text-primary"><Icon name="content_copy" size={14} /></span>
            Copy install cmd
          </button>
          <button
            class="flex items-center gap-2 p-2.5 rounded-xl bg-surface-container text-xs text-on-surface-variant
                   cursor-pointer border-none hover:bg-surface-container-high transition-colors"
            onclick={() => openUrl("https://github.com/schollz/croc")}
          >
            <span class="text-primary"><Icon name="open_in_new" size={14} /></span>
            croc GitHub
          </button>
        </div>

        <!-- How it works -->
        <div class="text-sm text-on-surface-variant leading-relaxed">
          <p class="font-medium text-on-surface mb-1">How it works</p>
          <ol class="list-decimal list-inside flex flex-col gap-1">
            <li>Add a contact with a shared code phrase</li>
            <li>Pick files and click Send — the code is used automatically</li>
            <li>On the other PC, files arrive instantly via LAN direct</li>
            <li>Files transfer directly, end-to-end encrypted via croc</li>
          </ol>
        </div>
      </div>
    {/if}
  </Card>

</div>

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
