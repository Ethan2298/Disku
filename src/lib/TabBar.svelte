<script lang="ts">
  export type Tab = "performance" | "rgb" | "apps-games" | "storage";

  interface Props {
    activeTab: Tab;
    onTabChange: (tab: Tab) => void;
  }

  let { activeTab, onTabChange }: Props = $props();

  const tabs: { label: string; key: Tab }[] = [
    { label: "Performance", key: "performance" },
    { label: "RGB", key: "rgb" },
    { label: "Apps & Games", key: "apps-games" },
    { label: "Storage", key: "storage" },
  ];
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="tab-bar" style="-webkit-app-region: no-drag">
  {#each tabs as tab}
    <button
      class="tab"
      class:active={activeTab === tab.key}
      onclick={() => onTabChange(tab.key)}
    >
      {tab.label}
    </button>
  {/each}
</div>

<style>
  .tab-bar {
    display: flex;
    align-items: flex-end;
    align-self: flex-end;
    margin-bottom: -2px;
  }

  .tab {
    padding: 6px 14px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    background: none;
    border: none;
    color: #606870;
    font-weight: 400;
  }

  .tab:hover:not(.active) {
    color: var(--text-secondary);
  }

  .tab.active {
    border: 2px solid var(--color-border);
    border-bottom: 2px solid var(--bg-primary);
    background: var(--bg-primary);
    color: var(--color-cyan);
    font-weight: 500;
  }
</style>
