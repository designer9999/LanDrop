<script lang="ts">
  interface Props {
    name: string;
    size?: number;
  }
  let { name, size = 24 }: Props = $props();
</script>

<span
  class="material-symbols-rounded leading-none"
  style="font-size: {size}px;"
>{name}</span>
