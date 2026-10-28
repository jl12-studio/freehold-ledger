<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import AppHeader from "./components/AppHeader.vue";
import EmptyState from "./components/EmptyState.vue";
import ErrorState from "./components/ErrorState.vue";
import LedgerFilters from "./components/LedgerFilters.vue";
import LedgerTable from "./components/LedgerTable.vue";
import {
  getLedgerInfo,
  listCategories,
  listIncomeSources,
  listTransactions,
  revealLedger,
} from "./lib/api";
import type { AppError, Category, LedgerInfo, Transaction } from "./lib/api";
import { DEFAULT_FILTERS, isDirty, toFilter } from "./lib/filters";
import type { FilterState } from "./lib/filters";

const info = ref<LedgerInfo | null>(null);
const error = ref<AppError | null>(null);
const showLoading = ref(false);
let loadingTimer: ReturnType<typeof setTimeout> | undefined;

const path = computed(() => info.value?.path ?? error.value?.path ?? null);
const state = computed<"loading" | "error" | "empty" | "ready">(() => {
  if (error.value) return "error";
  if (!info.value) return "loading";
  return info.value.transaction_count > 0 ? "ready" : "empty";
});

const filters = ref<FilterState>({ ...DEFAULT_FILTERS });
const rows = ref<Transaction[]>([]);
const categories = ref<Category[]>([]);
const sources = ref<string[]>([]);
const listError = ref<string | null>(null);
let fetchToken = 0;

async function refetch() {
  const token = ++fetchToken;
  try {
    const result = await listTransactions(toFilter(filters.value));
    if (token !== fetchToken) return;
    rows.value = result;
    listError.value = null;
  } catch (e) {
    if (token !== fetchToken) return;
    listError.value = asAppError(e).message;
  }
}

function clearFilters() {
  filters.value = { ...DEFAULT_FILTERS };
}

async function goTo(id: number) {
  // Make sure the target row can be on screen, then focus it.
  if (!filters.value.include_voided) {
    filters.value = { ...filters.value, include_voided: true };
    await refetch();
  }
  if (!rows.value.some((r) => r.id === id) && isDirty(filters.value)) {
    clearFilters();
    await refetch();
  }
  await nextTick();
  const el = document.getElementById(`tx-${id}`);
  el?.scrollIntoView?.({ block: "center" });
  el?.focus();
}

function reveal() {
  if (path.value) void revealLedger(path.value);
}

function asAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "message" in e) return e as AppError;
  return { kind: "other", message: String(e), path: "" };
}

onMounted(async () => {
  // Only show the loading caption if opening takes noticeable time.
  loadingTimer = setTimeout(() => (showLoading.value = true), 300);
  try {
    info.value = await getLedgerInfo();
    if (info.value.transaction_count > 0) {
      const [cats, srcs] = await Promise.all([listCategories(), listIncomeSources()]);
      categories.value = cats;
      sources.value = srcs;
      await refetch();
    }
  } catch (e) {
    error.value = asAppError(e);
  } finally {
    clearTimeout(loadingTimer);
  }
});

// Filter changes refetch (date typing is already debounced in the filter row).
watch(filters, () => void refetch(), { deep: true });

onBeforeUnmount(() => clearTimeout(loadingTimer));
</script>

<template>
  <AppHeader :path="path" @reveal="reveal" />
  <main>
    <p v-if="state === 'loading' && showLoading" class="max-w-measure mx-auto px-6 pt-16 text-ui text-muted">
      Opening your ledger…
    </p>
    <ErrorState v-else-if="state === 'error' && error" :error="error" @reveal="reveal" />
    <EmptyState v-else-if="state === 'empty'" @reveal="reveal" />
    <section v-else-if="state === 'ready' && info" class="max-w-page mx-auto px-6 pt-8 pb-16">
      <p class="sr-only">Your ledger is open.</p>
      <LedgerFilters v-model="filters" :categories="categories" :sources="sources" @clear="clearFilters" />
      <p v-if="listError" role="alert" class="pb-4 text-ui">{{ listError }}</p>
      <LedgerTable v-if="rows.length > 0" :rows="rows" :categories="categories" @goto="goTo" />
      <div v-else-if="!listError" class="pt-8">
        <p class="font-serif text-body">No entries match these filters.</p>
        <button
          type="button"
          class="mt-3 min-h-9 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
          @click="clearFilters"
        >Clear filters</button>
      </div>
    </section>
  </main>
</template>
