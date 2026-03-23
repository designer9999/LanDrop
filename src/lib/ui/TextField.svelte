<!--
  M3 Outlined Text Field
  - Height: 56dp (single-line), auto (multiline)
  - Border: outline 1dp default, on-surface hover, primary 2dp focused
  - Label floats from center to border notch
  - Shape: Extra Small (4dp) top corners
-->
<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Props {
    value?: string;
    label?: string;
    placeholder?: string;
    supportingText?: string;
    error?: boolean;
    disabled?: boolean;
    type?: string;
    multiline?: boolean;
    rows?: number;
    leadingIcon?: string;
    trailingIcon?: string;
    mono?: boolean;
    oninput?: (e: Event) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(""),
    label = "",
    placeholder = "",
    supportingText = "",
    error = false,
    disabled = false,
    type = "text",
    multiline = false,
    rows = 4,
    leadingIcon,
    trailingIcon,
    mono = false,
    oninput,
    onkeydown,
  }: Props = $props();

  let focused = $state(false);
  let hovered = $state(false);
  let hasValue = $derived(value.length > 0);
  let isFloating = $derived(focused || hasValue);
  const inputId = `tf-${Math.random().toString(36).slice(2, 8)}`;

  let showErrorIcon = $derived(error && !trailingIcon);
</script>

<div class="text-field-root relative" class:opacity-38={disabled}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="relative rounded-md border outline-solid outline-2 -outline-offset-2
           {error
             ? 'border-error outline-error'
             : focused
               ? 'border-primary outline-primary'
               : hovered
                 ? 'border-on-surface outline-transparent'
                 : 'border-outline outline-transparent'}"
    style="transition: border-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
                       outline-color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);"
    onpointerenter={() => (hovered = true)}
    onpointerleave={() => (hovered = false)}
  >
    {#if label}
      <label
        for={inputId}
        class="absolute pointer-events-none tf-label"
        class:left-3={!leadingIcon}
        class:left-11={!!leadingIcon}
        class:tf-label-float={isFloating}
        class:tf-label-rest={!isFloating}
        class:text-error={error}
        class:text-primary={focused && !error}
        class:text-on-surface-variant={!focused && !error}
      >
        {label}
      </label>
    {/if}

    {#if multiline}
      <textarea
        id={inputId}
        class="w-full bg-transparent text-base text-on-surface outline-none
               resize-y pl-4 pr-4 pt-5 pb-3
               disabled:cursor-not-allowed"
        class:font-mono={mono}
        {rows}
        placeholder={isFloating ? placeholder : ""}
        {disabled}
        bind:value
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
        {oninput}
        {onkeydown}
      ></textarea>
    {:else}
      <div class="flex items-center h-14">
        {#if leadingIcon}
          <span class="pl-3 text-on-surface-variant shrink-0">
            <Icon name={leadingIcon} size={24} />
          </span>
        {/if}

        <input
          id={inputId}
          class="flex-1 bg-transparent text-base text-on-surface outline-none
                 disabled:cursor-not-allowed h-full
                 pl-4
                 {trailingIcon || showErrorIcon ? 'pr-3' : 'pr-4'}"
          class:font-mono={mono}
          {type}
          placeholder={isFloating ? placeholder : ""}
          {disabled}
          bind:value
          onfocus={() => (focused = true)}
          onblur={() => (focused = false)}
          {oninput}
          {onkeydown}
        />

        {#if trailingIcon}
          <span class="pr-3 text-on-surface-variant shrink-0">
            <Icon name={trailingIcon} size={24} />
          </span>
        {:else if showErrorIcon}
          <span class="pr-3 text-error shrink-0">
            <Icon name="error" size={24} />
          </span>
        {/if}
      </div>
    {/if}
  </div>

  {#if supportingText}
    <p class="mt-1 ml-4 text-xs {error ? 'text-error' : 'text-on-surface-variant'}">
      {supportingText}
    </p>
  {/if}
</div>

<style>
  .tf-label {
    transition:
      top var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      font-size var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      padding var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .tf-label-float {
    top: -10px;
    font-size: 12px;
    line-height: 1;
    padding: 0 4px;
    background-color: var(--tf-bg, var(--color-surface));
  }
  .tf-label-rest {
    top: 16px;
    font-size: 16px;
    line-height: 1;
  }
</style>
