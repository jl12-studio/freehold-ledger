<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { exportTaxYear, listLedgerYears, revealFolder } from "../lib/api";
import type { AppError } from "../lib/api";

const props = defineProps<{ businessId: number; businessName: string }>();

const years = ref<number[]>([new Date().getFullYear()]);
const year = ref<number>(years.value[0]);
const working = ref(false);
const outcome = ref<{ ok: true; year: number; folder: string; business: string } | { ok: false; message: string } | null>(null);

let yearsToken = 0;
async function loadYears() {
  const token = ++yearsToken;
  try {
    const list = await listLedgerYears(props.businessId);
    if (token !== yearsToken) return;
    if (Array.isArray(list) && list.length > 0) {
      years.value = list;
      year.value = list[0];
    }
  } catch {
    // Keep the current year; the export itself still works.
  }
}

onMounted(loadYears);
watch(() => props.businessId, loadYears);

async function run() {
  if (working.value) return;
  working.value = true;
  outcome.value = null;
  const chosen = Number(year.value);
  try {
    const name = props.businessName;
    const result = await exportTaxYear(chosen, props.businessId);
    outcome.value = { ok: true, year: chosen, folder: result.folder, business: name };
  } catch (e) {
    const message =
      e && typeof e === "object" && "message" in e ? (e as AppError).message : String(e);
    // The core's own export errors already end "Nothing was created."; do not say it twice.
    const said = /nothing was (created|written)/i.test(message);
    outcome.value = { ok: false, message: said ? message : `${message} Nothing was written.` };
  } finally {
    working.value = false;
  }
}

function reveal() {
  if (outcome.value?.ok) void revealFolder(outcome.value.folder);
}
</script>

<template>
  <div class="flex items-center gap-2">
    <label for="export-year" class="text-caption text-muted">Tax year</label>
    <select
      id="export-year"
      v-model.number="year"
      class="min-h-8 border border-rule bg-paper px-2 text-ui text-ink"
    >
      <option v-for="y in years" :key="y" :value="y">{{ y }}</option>
    </select>
    <button
      type="button"
      class="min-h-8 shrink-0 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent aria-disabled:cursor-default aria-disabled:text-muted"
      :aria-disabled="working ? 'true' : undefined"
      @click="run"
    >
      {{ working ? "Exporting…" : "Export" }}
    </button>
  </div>
  <div
    v-if="outcome"
    role="status"
    aria-live="polite"
    class="basis-full flex flex-wrap items-center gap-x-4 gap-y-1 text-caption"
  >
    <template v-if="outcome.ok">
      <span class="min-w-0 break-words">Exported {{ outcome.business }} {{ outcome.year }} to {{ outcome.folder }}</span>
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="reveal"
      >
        Show in Finder
      </button>
    </template>
    <span v-else class="min-w-0 break-words">{{ outcome.message }}</span>
    <button
      type="button"
      class="min-h-8 cursor-pointer text-ui underline decoration-accent underline-offset-4 hover:text-accent"
      @click="outcome = null"
    >
      Dismiss
    </button>
  </div>
</template>
