<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { Category, PaymentMethod } from "../lib/api";
import { isDirty } from "../lib/filters";
import type { FilterState, Period } from "../lib/filters";

const props = defineProps<{
  modelValue: FilterState;
  categories: Category[];
  sources: string[];
  paymentMethods: PaymentMethod[];
  shown: number;
  voidedHidden: number;
}>();
const emit = defineEmits<{ "update:modelValue": [value: FilterState]; clear: [] }>();

const periods: { value: Period; label: string }[] = [
  { value: "", label: "All dates" },
  { value: "this_month", label: "This month" },
  { value: "last_month", label: "Last month" },
  { value: "this_quarter", label: "This quarter" },
  { value: "last_quarter", label: "Last quarter" },
  { value: "this_year", label: "This year" },
  { value: "last_year", label: "Last year" },
  { value: "custom", label: "Custom range…" },
];
const kinds = [
  { value: "", label: "All" },
  { value: "expense", label: "Expenses" },
  { value: "income", label: "Income" },
];

const expenseCats = computed(() => props.categories.filter((c) => c.kind === "expense"));
const incomeCats = computed(() => props.categories.filter((c) => c.kind === "income"));
const activeMethods = computed(() => props.paymentMethods.filter((m) => !m.retired));
const retiredMethods = computed(() => props.paymentMethods.filter((m) => m.retired));
function methodLabel(m: PaymentMethod): string {
  return m.last4 ? `${m.nickname} (${m.last4})` : m.nickname;
}
const dirty = computed(() => isDirty(props.modelValue));
const countText = computed(
  () =>
    `${props.shown} ${props.shown === 1 ? "entry" : "entries"}` +
    (props.voidedHidden > 0 ? ` · ${props.voidedHidden} voided hidden` : ""),
);

function update(patch: Partial<FilterState>) {
  emit("update:modelValue", { ...props.modelValue, ...patch });
}

function optionLabel(c: Category): string {
  return c.schedule_c_line ? `${c.label} (line ${c.schedule_c_line})` : c.label;
}

// Date typing is debounced; everything else applies at once.
const from = ref(props.modelValue.from);
const to = ref(props.modelValue.to);
let timer: ReturnType<typeof setTimeout> | undefined;
watch(() => props.modelValue.from, (v) => (from.value = v));
watch(() => props.modelValue.to, (v) => (to.value = v));
function onDate(key: "from" | "to", value: string) {
  (key === "from" ? from : to).value = value;
  clearTimeout(timer);
  timer = setTimeout(() => update({ from: from.value, to: to.value }), 250);
}
onBeforeUnmount(() => clearTimeout(timer));

function onPeriod(value: string) {
  clearTimeout(timer);
  update(value === "custom" ? { period: "custom" } : { period: value as Period, from: "", to: "" });
}

function onKind(value: string) {
  update(value === "expense" ? { kind: "expense", source: "" } : { kind: value as "" | "income" });
}

const active = (v: string) => (v !== "" ? "true" : undefined);
</script>

<template>
  <form role="search" aria-label="Filter entries" class="flex flex-wrap items-center gap-2 pb-4" @submit.prevent>
    <select
      aria-label="Dates"
      class="control"
      :data-active="active(modelValue.period)"
      :value="modelValue.period"
      @change="onPeriod(($event.target as HTMLSelectElement).value)"
    >
      <option v-for="p in periods" :key="p.value" :value="p.value">{{ p.label }}</option>
    </select>

    <template v-if="modelValue.period === 'custom'">
      <input type="date" aria-label="From" class="control w-[9.75rem]" :value="from" @input="onDate('from', ($event.target as HTMLInputElement).value)" />
      <span class="label">to</span>
      <input type="date" aria-label="To" class="control w-[9.75rem]" :value="to" @input="onDate('to', ($event.target as HTMLInputElement).value)" />
    </template>

    <fieldset>
      <legend class="sr-only">Type</legend>
      <div class="inline-flex h-8 rounded-sm border border-rule p-0.5">
        <label
          v-for="k in kinds"
          :key="k.value"
          class="inline-flex cursor-pointer items-center rounded-[2px] px-2.5 text-ui text-muted has-[:checked]:bg-surface has-[:checked]:font-[560] has-[:checked]:text-fg has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-focus"
        >
          <input type="radio" name="kind" class="sr-only" :value="k.value" :checked="modelValue.kind === k.value" @change="onKind(k.value)" />
          {{ k.label }}
        </label>
      </div>
    </fieldset>

    <select
      aria-label="Category"
      class="control max-w-[12rem]"
      :data-active="active(modelValue.category)"
      :value="modelValue.category"
      @change="update({ category: ($event.target as HTMLSelectElement).value })"
    >
      <option value="">All categories</option>
      <optgroup label="Expense">
        <option v-for="c in expenseCats" :key="c.key" :value="c.key">{{ optionLabel(c) }}</option>
      </optgroup>
      <optgroup label="Income">
        <option v-for="c in incomeCats" :key="c.key" :value="c.key">{{ optionLabel(c) }}</option>
      </optgroup>
    </select>

    <select
      v-if="modelValue.kind !== 'expense'"
      aria-label="Source"
      class="control"
      :data-active="active(modelValue.source)"
      :value="modelValue.source"
      @change="update({ source: ($event.target as HTMLSelectElement).value })"
    >
      <option value="">All sources</option>
      <option v-for="s in sources" :key="s" :value="s">{{ s }}</option>
    </select>

    <select
      v-if="paymentMethods.length > 0"
      aria-label="Payment method"
      class="control"
      :data-active="active(modelValue.payment_method)"
      :value="modelValue.payment_method"
      @change="update({ payment_method: ($event.target as HTMLSelectElement).value })"
    >
      <option value="">Any payment method</option>
      <option v-for="m in activeMethods" :key="m.id" :value="String(m.id)">{{ methodLabel(m) }}</option>
      <optgroup v-if="retiredMethods.length > 0" label="Retired">
        <option v-for="m in retiredMethods" :key="m.id" :value="String(m.id)">{{ methodLabel(m) }}</option>
      </optgroup>
      <option value="none">Not set</option>
    </select>

    <label class="label inline-flex h-8 items-center gap-2">
      <input type="checkbox" class="size-4 accent-accent" :checked="modelValue.include_voided" @change="update({ include_voided: ($event.target as HTMLInputElement).checked })" />
      Show voided
    </label>

    <div class="ml-auto flex items-center gap-3">
      <span class="label" aria-live="polite">{{ countText }}</span>
      <button v-if="dirty" type="button" class="link text-caption" @click="emit('clear')">Clear all</button>
    </div>
  </form>
</template>
