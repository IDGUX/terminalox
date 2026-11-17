<script lang="ts">
  import { secretPrompt } from "$lib/stores/secret.svelte";

  const req = $derived(secretPrompt.current?.request ?? null);
  const isPassphrase = $derived(req?.kind === "passphrase");

  let value = $state("");
  let remember = $state(false);

  // Fresh fields for every new request.
  $effect(() => {
    if (req) {
      value = "";
      remember = false;
    }
  });

  function submit(e: Event) {
    e.preventDefault();
    secretPrompt.submit(value, remember);
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && req && secretPrompt.cancel()} />

{#if req}
  <div class="overlay">
    <div role="dialog" aria-modal="true" aria-label="Passwort-Abfrage">
    <form class="dialog" onsubmit={submit}>
      <h3>
        {#if isPassphrase}Passphrase für Key „{req.label}“
        {:else}Passwort für „{req.label}“{/if}
      </h3>
      {#if req.wrong}
        <p class="error">
          {isPassphrase ? "Die Passphrase war falsch." : "Das Passwort wurde vom Server abgelehnt."}
          Bitte erneut eingeben.
        </p>
      {:else}
        <p>
          {#if isPassphrase}
            Der Key ist mit einer Passphrase geschützt.
          {:else}
            Der Server verlangt ein Passwort.
          {/if}
        </p>
      {/if}

      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="password"
        bind:value
        autofocus
        autocomplete="off"
        placeholder={isPassphrase ? "Passphrase" : "SSH-Passwort"}
      />
      <label class="check">
        <input type="checkbox" bind:checked={remember} />
        Im Schlüsselbund des Betriebssystems merken
      </label>
      <p class="hint">
        Ohne Häkchen gilt die Eingabe nur, bis terminalox beendet wird.
        {#if !isPassphrase}
          Noch bequemer: „SSH-Key deployen“ — danach braucht dieser Server kein Passwort mehr.
        {/if}
      </p>

      {#if secretPrompt.error}<p class="error">{secretPrompt.error}</p>{/if}

      <div class="buttons">
        <button type="button" onclick={() => secretPrompt.cancel()}>Abbrechen</button>
        <button type="submit" class="primary" disabled={!value || secretPrompt.busy}>
          {secretPrompt.busy ? "Prüfe …" : "Weiter"}
        </button>
      </div>
    </form>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .dialog {
    background: #1f232b;
    color: #e5e7eb;
    border-radius: 10px;
    padding: 20px;
    width: 400px;
    box-shadow: 0 12px 40px rgba(0,0,0,0.5);
    border: 1px solid #2a2e37;
  }
  h3 { margin: 0 0 10px; font-size: 16px; }
  p { font-size: 13px; line-height: 1.5; margin: 0 0 10px; color: #d1d5db; }
  input[type="password"] {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    background: #11141a;
    border: 1px solid #2a2e37;
    color: #e5e7eb;
    border-radius: 6px;
    font-size: 14px;
  }
  .check { display: flex; gap: 8px; align-items: center; font-size: 13px; margin: 10px 0 6px; color: #d1d5db; }
  .hint { font-size: 11px; color: #6b7280; }
  .error { color: #f87171; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  button {
    padding: 8px 14px;
    border-radius: 6px;
    border: 1px solid #2a2e37;
    background: #2a2e37;
    color: #e5e7eb;
    cursor: pointer;
    font-size: 13px;
  }
  button.primary { background: #2563eb; border-color: #2563eb; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
