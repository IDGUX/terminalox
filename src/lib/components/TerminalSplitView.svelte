<script lang="ts">
  import TerminalView from "./TerminalView.svelte";
  import type { Server } from "$lib/types";

  let { server }: { server: Server } = $props();

  const MAX_PANELS = 4;
  let nextId = 1;
  let panels = $state<{ id: number }[]>([{ id: 0 }]);

  function addPanel() {
    if (panels.length >= MAX_PANELS) return;
    panels = [...panels, { id: nextId++ }];
  }

  function removePanel(id: number) {
    if (panels.length <= 1) return;
    panels = panels.filter((p) => p.id !== id);
  }
</script>

<div class="grid count-{panels.length}">
  {#each panels as p (p.id)}
    <TerminalView
      {server}
      onSplit={addPanel}
      onClose={() => removePanel(p.id)}
      canSplit={panels.length < MAX_PANELS}
      canClose={panels.length > 1}
    />
  {/each}
</div>

<style>
  .grid {
    flex: 1;
    display: grid;
    gap: 4px;
    padding: 4px;
    min-height: 0;
    min-width: 0;
  }
  .count-1 {
    grid-template-columns: 1fr;
    grid-template-rows: 1fr;
  }
  .count-2 {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr;
  }
  .count-3 {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
  }
  .count-3 > :nth-child(3) {
    grid-column: span 2;
  }
  .count-4 {
    grid-template-columns: 1fr 1fr;
    grid-template-rows: 1fr 1fr;
  }
</style>
