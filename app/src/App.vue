<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import ConnectAgentPanel from "./components/ConnectAgentPanel.vue";
import AppHeader from "./components/AppHeader.vue";
import EmptyState from "./components/EmptyState.vue";
import ErrorState from "./components/ErrorState.vue";
import LedgerFilters from "./components/LedgerFilters.vue";
import LedgerTable from "./components/LedgerTable.vue";
import {
  getLedgerInfo,
  listBusinesses,
  listCategories,
  listIncomeSources,
  listPaymentMethods,
  listTransactions,
  revealLedger,
} from "./lib/api";
import type { AppError, Business, Category, LedgerInfo, PaymentMethod, Transaction } from "./lib/api";
import { DEFAULT_FILTERS, isDirty, toFilter } from "./lib/filters";
import type { FilterState } from "./lib/filters";
import { loadViewBusiness, pickBusiness, saveViewBusiness } from "./lib/businessView";

const info = ref<LedgerInfo | null>(null);
const error = ref<AppError | null>(null);
const showLoading = ref(false);
let loadingTimer: ReturnType<typeof setTimeout> | undefined;

const path = computed(() => info.value?.path ?? error.value?.path ?? null);
const businesses = ref<Business[]>([]);
const selectedId = ref<number | null>(null);
const selected = computed(() => businesses.value.find((b) => b.id === selectedId.value) ?? null);
const currentName = computed(
  () => businesses.value.find((b) => b.id === info.value?.current_business_id)?.name ?? null,
);
const state = computed<"loading" | "error" | "empty" | "ready">(() => {
  if (error.value) return "error";
  if (!info.value) return "loading";
  return selected.value && selected.value.transaction_count > 0 ? "ready" : "empty";
});

const filters = ref<FilterState>({ ...DEFAULT_FILTERS });
const rows = ref<Transaction[]>([]);
const categories = ref<Category[]>([]);
const sources = ref<string[]>([]);
const paymentMethods = ref<PaymentMethod[]>([]);
const listError = ref<string | null>(null);
let fetchToken = 0;
let inflight: Promise<void> = Promise.resolve();

function refetch(): Promise<void> {
  inflight = fetchRows();
  return inflight;
}

/** Wait for the filter watcher's refetch, and any that superseded it, to land. */
async function settle() {
  await nextTick();
  let p: Promise<void>;
  do {
    p = inflight;
    await p;
  } while (p !== inflight);
}

async function fetchRows() {
  const token = ++fetchToken;
  if (selectedId.value === null) return;
  try {
    const result = await listTransactions({ ...toFilter(filters.value), business_id: selectedId.value });
    if (token !== fetchToken) return;
    rows.value = result;
    listError.value = null;
  } catch (e) {
    if (token !== fetchToken) return;
    listError.value = asAppError(e).message;
  }
}

let optionsToken = 0;
/** The business's own filter options. A reply for a business that is no longer selected is dropped. */
async function loadOptions(id: number) {
  const token = ++optionsToken;
  const [srcs, pms] = await Promise.all([listIncomeSources(id), listPaymentMethods(id)]);
  if (token !== optionsToken) return;
  sources.value = srcs;
  paymentMethods.value = pms;
}

async function selectBusiness(id: number) {
  if (id === selectedId.value || !path.value) return;
  selectedId.value = id;
  saveViewBusiness(path.value, id);
  // Rows are refetched by the filters watcher (always a new object here).
  filters.value = { ...filters.value, source: "", payment_method: "" };
  try {
    await loadOptions(id);
  } catch (e) {
    listError.value = asAppError(e).message;
  }
}

function clearFilters() {
  filters.value = { ...DEFAULT_FILTERS };
}

async function goTo(id: number) {
  // Make sure the target row can be on screen, then focus it.
  // The filters watcher does the refetch; a second one here would be discarded as stale.
  if (!filters.value.include_voided) {
    filters.value = { ...filters.value, include_voided: true };
    await settle();
  }
  if (!rows.value.some((r) => r.id === id) && isDirty(filters.value)) {
    clearFilters();
    await settle();
  }
  await nextTick();
  const el = document.getElementById(`tx-${id}`);
  el?.scrollIntoView?.({ block: "center" });
  el?.focus();
}

const connectPanel = ref<InstanceType<typeof ConnectAgentPanel> | null>(null);
function connect() {
  void connectPanel.value?.open();
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
    const opened = await getLedgerInfo();
    const list = await listBusinesses();
    const pick = pickBusiness(list, loadViewBusiness(opened.path), opened.current_business_id);
    businesses.value = list;
    selectedId.value = pick?.id ?? null;
    info.value = opened;
    if (pick) {
      const [cats] = await Promise.all([listCategories(), loadOptions(pick.id)]);
      categories.value = cats;
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
  <AppHeader
    :path="path"
    :show-export="state === 'ready' || state === 'empty'"
    :businesses="businesses"
    :selected-id="selectedId"
    :current-id="info?.current_business_id ?? null"
    @select="selectBusiness"
    @reveal="reveal"
    @connect="connect"
  />
  <ConnectAgentPanel ref="connectPanel" />
  <main>
    <p v-if="state === 'loading' && showLoading" class="max-w-measure mx-auto px-6 pt-16 text-ui text-muted">
      Opening your ledger…
    </p>
    <ErrorState v-else-if="state === 'error' && error" :error="error" @reveal="reveal" />
    <EmptyState
      v-else-if="state === 'empty'"
      :business-name="selected?.name ?? 'this ledger'"
      :current-name="currentName"
      :is-current="selected !== null && selected.id === info?.current_business_id"
      @reveal="reveal" @connect="connect" />
    <section v-else-if="state === 'ready' && info" class="max-w-page mx-auto px-6 pt-8 pb-16">
      <p class="sr-only">Your ledger is open.</p>
      <LedgerFilters v-model="filters" :categories="categories" :sources="sources" :payment-methods="paymentMethods" @clear="clearFilters" />
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
