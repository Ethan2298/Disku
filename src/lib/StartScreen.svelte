<script lang="ts">
  interface Props {
    onSelect: (choice: string) => void;
  }

  let { onSelect }: Props = $props();

  const menuItems = [
    { label: "Scan Volume", key: "volume" },
    { label: "Scan Directory", key: "directory" },
    { label: "Quit", key: "quit" },
  ];

  let selectedIndex: number = $state(0);

  function handleKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowUp":
      case "k":
        e.preventDefault();
        if (selectedIndex > 0) selectedIndex--;
        break;
      case "ArrowDown":
      case "j":
        e.preventDefault();
        if (selectedIndex < menuItems.length - 1) selectedIndex++;
        break;
      case "Enter":
        e.preventDefault();
        onSelect(menuItems[selectedIndex].key);
        break;
      case "q":
      case "Escape":
        e.preventDefault();
        onSelect("quit");
        break;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="start-screen">
  <pre class="ascii-art"
    >{`    ██████╗ ██╗███████╗██╗  ██╗██╗   ██╗
    ██╔══██╗██║██╔════╝██║ ██╔╝██║   ██║
    ██║  ██║██║███████╗█████╔╝ ██║   ██║
    ██║  ██║██║╚════██║██╔═██╗ ██║   ██║
    ██████╔╝██║███████║██║  ██╗╚██████╔╝
    ╚═════╝ ╚═╝╚══════╝╚═╝  ╚═╝ ╚═════╝`}</pre>

  <p class="tagline">Fast disk usage analyzer</p>

  <div class="menu">
    {#each menuItems as item, i}
      <button
        class="menu-item"
        class:selected={i === selectedIndex}
        onmouseenter={() => (selectedIndex = i)}
        onclick={() => onSelect(item.key)}
      >
        <span class="indicator">{i === selectedIndex ? "▸" : " "}</span>
        <span class="label">{item.label}</span>
      </button>
    {/each}
  </div>

  <p class="hint">↑/↓ navigate · Enter select · q quit</p>
</div>

<style>
  .start-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    gap: 16px;
  }

  .ascii-art {
    color: var(--color-cyan);
    font-size: 14px;
    line-height: 1.2;
    text-align: center;
  }

  .tagline {
    color: var(--text-secondary);
    font-size: 13px;
    margin-bottom: 16px;
  }

  .menu {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 200px;
  }

  .menu-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 16px;
    background: none;
    border: none;
    cursor: pointer;
    font-family: inherit;
    font-size: 14px;
    color: var(--text-secondary);
    text-align: left;
  }

  .menu-item.selected {
    color: white;
    font-weight: bold;
  }

  .menu-item.selected .indicator {
    color: var(--color-accent);
  }

  .indicator {
    width: 12px;
    font-weight: bold;
  }

  .hint {
    color: var(--text-muted);
    font-size: 12px;
    margin-top: 16px;
  }
</style>
