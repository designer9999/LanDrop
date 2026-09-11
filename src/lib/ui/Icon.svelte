<script lang="ts">
  interface Props {
    name: string;
    size?: number;
  }
  let { name, size = 24 }: Props = $props();
</script>

<span
  class="material-symbols-rounded leading-none"
  aria-hidden="true"
  style="font-size: {size}px; display: inline-flex; align-items: center; justify-content: center; width: {size}px; height: {size}px; vertical-align: middle;"
  >{name}</span
>
