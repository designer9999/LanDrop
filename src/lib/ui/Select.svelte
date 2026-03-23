<!--
  M3-styled Select (dropdown)
  - Appearance matches TextField (outlined, same height, same border behavior)
  - Custom arrow icon
-->
<script lang="ts">
  import Icon from "./Icon.svelte";

  interface Option {
    value: string;
    label: string;
  }

  interface Props {
    value?: string;
    label?: string;
    options: Option[];
    supportingText?: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(""),
    label = "",
    options,
    supportingText = "",
    disabled = false,
    onchange,
  }: Props = $props();

  let focused = $state(false);
  let hovered = $state(false);
  let hasValue = $derived(value.length > 0);
  let isFloating = $derived(focused || hasValue);
  const selectId = `sel-${Math.random().toString(36).slice(2, 8)}`;

  function handleChange(e: Event) {
    value = (e.target as HTMLSelectElement).value;
    onchange?.(value);
  }
</script>

<div class="relative" class:opacity-38={disabled}>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="relative rounded-md border outline-solid outline-2 -outline-offset-2
           {focused
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
        for={selectId}
        class="absolute pointer-events-none left-3 sel-label"
        class:sel-label-float={isFloating}
        class:sel-label-rest={!isFloating}
        class:text-primary={focused}
        class:text-on-surface-variant={!focused}
      >
        {label}
      </label>
    {/if}

    <div class="flex items-center h-14">
      <select
        id={selectId}
        class="sel-input"
        {disabled}
        onfocus={() => (focused = true)}
        onblur={() => (focused = false)}
        onchange={handleChange}
        {value}
      >
        {#each options as opt (opt.value)}
          <option value={opt.value} selected={opt.value === value}>{opt.label}</option>
        {/each}
      </select>
      <span class="absolute right-3 pointer-events-none text-on-surface-variant">
        <Icon name="arrow_drop_down" size={20} />
      </span>
    </div>
  </div>

  {#if supportingText}
    <p class="mt-1 ml-4 text-xs text-on-surface-variant">{supportingText}</p>
  {/if}
</div>

<style>
  .sel-input {
    width: 100%;
    height: 100%;
    background: transparent;
    color: var(--md-sys-color-on-surface);
    font-size: 1rem;
    font-family: inherit;
    cursor: pointer;
    appearance: none;
    border: none;
    outline: none;
    padding: 8px 40px 0 16px;
  }
  .sel-input option {
    background: var(--md-sys-color-surface-container);
    color: var(--md-sys-color-on-surface);
  }
  .sel-label {
    transition:
      top var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      font-size var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects),
      padding var(--md-spring-fast-spatial-dur) var(--md-spring-fast-spatial),
      color var(--md-spring-fast-effects-dur) var(--md-spring-fast-effects);
  }
  .sel-label-float {
    top: -10px;
    font-size: 12px;
    line-height: 1;
    padding: 0 4px;
    background-color: var(--tf-bg, var(--color-surface));
  }
  .sel-label-rest {
    top: 16px;
    font-size: 16px;
    line-height: 1;
  }
</style>
