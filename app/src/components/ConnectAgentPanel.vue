<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getConnectionInfo } from "../lib/api";
import type { ConnectionInfo } from "../lib/api";

const dialog = ref<HTMLDialogElement | null>(null);
const info = ref<ConnectionInfo | null>(null);
const loadError = ref<string | null>(null);
const copied = ref(false);
const announcement = ref("");
let opener: HTMLElement | null = null;
let copiedTimer: ReturnType<typeof setTimeout> | undefined;

async function open() {
  opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const el = dialog.value;
  if (!el) return;
  if (typeof el.showModal === "function") el.showModal();
  else el.setAttribute("open", "");
  try {
    info.value = await getConnectionInfo();
    loadError.value = null;
  } catch (e) {
    loadError.value =
      e && typeof e === "object" && "message" in e ? String((e as { message: unknown }).message) : String(e);
  }
}

function close() {
  const el = dialog.value;
  if (!el) return;
  if (typeof el.close === "function") el.close();
  else el.removeAttribute("open");
  onClosed();
}

/** Fires for the Close button and for Esc. */
function onClosed() {
  opener?.focus();
  opener = null;
}

async function copy(text: string) {
  clearTimeout(copiedTimer);
  try {
    await writeText(text);
    copied.value = true;
    announcement.value = "Copied the message";
    copiedTimer = setTimeout(() => {
      copied.value = false;
      announcement.value = "";
    }, 2000);
  } catch {
    copied.value = false;
    announcement.value = "Could not copy. Select the text and copy it by hand.";
  }
}

onBeforeUnmount(() => clearTimeout(copiedTimer));
defineExpose({ open });
</script>

<template>
  <dialog
    ref="dialog"
    aria-labelledby="connect-title"
    class="m-auto w-[min(44rem,calc(100vw-2rem))] max-h-[calc(100vh-2rem)] overflow-y-auto border border-rule bg-bg p-6 text-fg backdrop:bg-fg/40"
    @close="onClosed"
  >
    <h2 id="connect-title" class="font-serif text-h2 font-semibold">Connect your agent</h2>
    <p class="mt-3 font-serif text-body">
      Copy this message and send it to your AI agent. It will set itself up.
    </p>
    <p class="mt-1 text-caption text-muted">
      Works with AI apps on this computer that support MCP. Browser and phone chats can't connect to
      a local app yet.
    </p>

    <p v-if="loadError" role="alert" class="mt-6 text-ui">{{ loadError }}</p>
    <p v-else-if="!info" class="mt-6 text-ui text-muted">Looking up the settings…</p>

    <template v-if="info">
      <pre
        class="mt-4 max-h-72 overflow-auto whitespace-pre-wrap break-words rounded-sm bg-surface p-3 font-mono text-caption select-text"
        tabindex="0"
        aria-label="Message for your agent"
      >{{ info.agent_message }}</pre>
      <button
        type="button"
        class="btn btn-primary mt-2"
        @click="copy(info.agent_message)"
      >
        {{ copied ? "Copied" : "Copy" }}
      </button>
      <p v-if="!info.bundled" class="mt-6 text-caption text-muted">
        You're running a development build. This path changes once the app is installed in
        Applications.
      </p>
    </template>

    <p class="sr-only" aria-live="polite">{{ announcement }}</p>

    <div class="mt-6 flex justify-end">
      <button
        type="button"
        class="btn"
        @click="close"
      >
        Close
      </button>
    </div>
  </dialog>
</template>
