<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { Category } from "../lib/api";
import { isDirty } from "../lib/filters";
import type { FilterState } from "../lib/filters";

const props = defineProps<{ modelValue: FilterState; categories: Category[]; sources: string[] }>();
const emit = defineEmits<{ "update:modelValue": [value: FilterState]; clear: [] }>();

const expenseCats = computed(() => props.categories.filter((c) => c.kind === "expense"));
const incomeCats = computed(() => props.categories.filter((c) => c.kind === "income"));
const dirty = computed(() => isDirty(props.modelValue));

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

function onKind(value: string) {
  update(value === "expense" ? { kind: "expense", source: "" } : { kind: value as "" | "income" });
}

const control =
  "min-h-9 rounded-sm border border-rule bg-bg px-2 text-ui text-fg disabled:opacity-60";
const labelCls = "flex flex-col gap-1 text-caption text-muted";
</script>

<template>
  <form class="flex flex-wrap items-end gap-x-4 gap-y-3 pb-4" @submit.prevent>
    <label :class="labelCls"><span>From</span>
      <input name="from" type="date" :class="control" :value="from" @input="onDate('from', ($event.target as HTMLInputElement).value)" />
    </label>
    <label :class="labelCls"><span>To</span>
      <input name="to" type="date" :class="control" :value="to" @input="onDate('to', ($event.target as HTMLInputElement).value)" />
    </label>
    <label :class="labelCls"><span>Kind</span>
      <select name="kind" :class="control" :value="modelValue.kind" @change="onKind(($event.target as HTMLSelectElement).value)">
        <option value="">All</option>
        <option value="expense">Expenses</option>
        <option value="income">Income</option>
      </select>
    </label>
    <label :class="labelCls"><span>Category</span>
      <select name="category" :class="control" :value="modelValue.category" @change="update({ category: ($event.target as HTMLSelectElement).value })">
        <option value="">All</option>
        <optgroup label="Expense">
          <option v-for="c in expenseCats" :key="c.key" :value="c.key">{{ optionLabel(c) }}</option>
        </optgroup>
        <optgroup label="Income">
          <option v-for="c in incomeCats" :key="c.key" :value="c.key">{{ optionLabel(c) }}</option>
        </optgroup>
      </select>
    </label>
    <label :class="labelCls"><span>Source</span>
      <select name="source" :class="control" :disabled="modelValue.kind === 'expense'" :value="modelValue.source" @change="update({ source: ($event.target as HTMLSelectElement).value })">
        <option value="">All</option>
        <option v-for="s in sources" :key="s" :value="s">{{ s }}</option>
      </select>
    </label>
    <label class="flex min-h-9 items-center gap-2 text-caption text-muted">
      <input type="checkbox" class="size-4 accent-accent" :checked="modelValue.include_voided" @change="update({ include_voided: ($event.target as HTMLInputElement).checked })" />
      <span>Show voided entries</span>
    </label>
    <button
      v-if="dirty"
      type="button"
      class="min-h-9 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
      @click="emit('clear')"
    >Clear filters</button>
  </form>
</template>
