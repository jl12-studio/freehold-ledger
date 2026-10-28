<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getConnectionInfo } from "../lib/api";
import type { ConnectionInfo } from "../lib/api";

const dialog = ref<HTMLDialogElement | null>(null);
const info = ref<ConnectionInfo | null>(null);
const loadError = ref<string | null>(null);
const copied = ref<string | null>(null);
const announcement = ref("");
let opener: HTMLElement | null = null;
let copiedTimer: ReturnType<typeof setTimeout> | undefined;

const sections = computed(() =>
  info.value
    ? [
        {
          key: "desktop",
          title: "Claude Desktop",
          steps: [
            "In Claude Desktop, open Settings, then Developer, then Edit Config.",
            "Paste this into claude_desktop_config.json (merge it with what is there), save, and restart Claude.",
          ],
          code: info.value.claude_desktop_json,
        },
        {
          key: "code",
          title: "Claude Code",
          steps: ["Run this command in Terminal."],
          code: info.value.claude_code_command,
        },
        {
          key: "other",
          title: "Other agents",
          steps: ["Use this in any agent that supports MCP."],
          code: info.value.generic,
        },
      ]
    : [],
);

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

async function copy(key: string, text: string, title: string) {
  clearTimeout(copiedTimer);
  try {
    await writeText(text);
    copied.value = key;
    announcement.value = `Copied the ${title} text`;
    copiedTimer = setTimeout(() => {
      copied.value = null;
      announcement.value = "";
    }, 2000);
  } catch {
    copied.value = null;
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
      Your agent keeps the books by talking to Freehold Ledger over MCP. Nothing goes over the
      network: your agent starts the ledger's server on this computer, and it reads and writes only
      your ledger file.
    </p>

    <p v-if="loadError" role="alert" class="mt-6 text-ui">{{ loadError }}</p>
    <p v-else-if="!info" class="mt-6 text-ui text-muted">Looking up the settings…</p>

    <section v-for="s in sections" :key="s.key" class="mt-6 border-t border-rule pt-4">
      <h3 class="font-sans text-h4 font-semibold">{{ s.title }}</h3>
      <p v-for="step in s.steps" :key="step" class="mt-1 text-ui">{{ step }}</p>
      <pre
        class="mt-3 overflow-x-auto rounded-sm bg-surface p-3 font-mono text-caption"
        tabindex="0"
        :aria-label="`${s.title} settings`"
      >{{ s.code }}</pre>
      <button
        type="button"
        class="mt-2 min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="copy(s.key, s.code, s.title)"
      >
        {{ copied === s.key ? "Copied" : "Copy" }}
      </button>
    </section>

    <template v-if="info">
      <p v-if="!info.bundled" class="mt-6 text-caption text-muted">
        You're running a development build. This path changes once the app is installed in
        Applications.
      </p>
      <p class="mt-3 text-caption text-muted">
        Then ask your agent: 'What's in my ledger?' It should answer from your file.
      </p>
    </template>

    <p class="sr-only" aria-live="polite">{{ announcement }}</p>

    <div class="mt-6 flex justify-end">
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="close"
      >
        Close
      </button>
    </div>
  </dialog>
</template>
