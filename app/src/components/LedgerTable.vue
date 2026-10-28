<script setup lang="ts">
import { onBeforeUnmount } from "vue";
import { formatAmount, formatDate } from "../lib/api";
import type { Category, Transaction } from "../lib/api";
import TransactionTrail from "./TransactionTrail.vue";

defineProps<{ rows: Transaction[]; categories: Category[] }>();
const emit = defineEmits<{ goto: [id: number] }>();

function hasTrail(t: Transaction): boolean {
  return !!t.void || t.replaced_by_id !== null || t.replaces_id !== null || t.classification_history.length > 0;
}

const FLASH_MS = 1200;
const timers = new Map<HTMLElement, ReturnType<typeof setTimeout>>();

function flash(e: FocusEvent) {
  const row = e.currentTarget as HTMLElement;
  if (e.target !== row) return;
  const pending = timers.get(row);
  if (pending) clearTimeout(pending);
  row.setAttribute("data-flash", "");
  timers.set(
    row,
    setTimeout(() => {
      row.removeAttribute("data-flash");
      timers.delete(row);
    }, FLASH_MS),
  );
}

onBeforeUnmount(() => {
  for (const timer of timers.values()) clearTimeout(timer);
  timers.clear();
});

const th = "label border-b border-rule py-2 pr-4 font-[560]";
</script>

<template>
  <div class="overflow-x-auto">
    <table class="w-full min-w-[42rem] table-fixed border-collapse text-left font-sans text-ui">
      <caption class="sr-only">Ledger entries</caption>
      <colgroup>
        <col class="w-[6.5rem]" />
        <col />
        <col class="w-[10.5rem] lg:w-[12rem]" />
        <col class="w-[9.5rem] lg:w-[11rem]" />
        <col class="w-[7.5rem]" />
      </colgroup>
      <thead>
        <tr>
          <th scope="col" :class="th">Date</th>
          <th scope="col" :class="th">Description</th>
          <th scope="col" :class="th">Category</th>
          <th scope="col" :class="th">Paid with</th>
          <th scope="col" :class="[th, 'text-right pr-0']">Amount</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="t in rows" :key="t.id">
          <tr
            :id="`tx-${t.id}`"
            tabindex="-1"
            :aria-describedby="hasTrail(t) ? `trail-${t.id}` : undefined"
            :class="hasTrail(t) ? '' : 'border-b border-rule'"
            @focus="flash"
          >
            <td class="whitespace-nowrap py-2 pr-4 align-baseline tabular-nums text-muted">{{ formatDate(t.occurred_on) }}</td>
            <td class="py-2 pr-4 align-baseline">
              <div class="flex min-w-0 items-baseline gap-2">
                <span :class="['truncate', t.void ? 'line-through text-muted' : '']" :title="t.description">{{ t.description }}</span>
                <span v-if="t.kind === 'income' && t.income_source" class="shrink-0 text-muted">from {{ t.income_source }}</span>
                <span v-if="t.is_asset" class="chip chip-muted shrink-0">Asset</span>
                <span v-if="t.void" class="chip chip-muted shrink-0">Voided</span>
                <span v-else-if="t.replaces_id !== null" class="chip chip-muted shrink-0">Correction</span>
              </div>
            </td>
            <td :class="['truncate py-2 pr-4 align-baseline', t.void ? 'text-muted' : '']">
              <template v-if="t.category === 'uncategorized'">
                <span class="font-[560]">Needs a category</span>
              </template>
              <template v-else>
                {{ t.category_label }}
                <span v-if="t.schedule_c_line" class="ml-1.5 text-caption text-muted tabular-nums">line {{ t.schedule_c_line }}</span>
              </template>
            </td>
            <td :class="['truncate py-2 pr-4 align-baseline', t.void ? 'text-muted' : '']">
              <template v-if="t.payment_method">
                {{ t.payment_method.nickname }}
                <span v-if="t.payment_method.last4" class="ml-1.5 text-caption text-muted tabular-nums"><span class="sr-only">ending </span><span aria-hidden="true">··</span>{{ t.payment_method.last4 }}</span>
                <span v-if="t.payment_method.retired" class="chip chip-muted ml-1.5">Retired</span>
              </template>
              <span v-else class="text-muted">Not set</span>
            </td>
            <td
              :class="[
                'whitespace-nowrap py-2 pr-0 text-right align-baseline tabular-nums',
                t.void ? 'line-through text-muted' : t.kind === 'income' ? 'text-income' : 'text-fg',
              ]"
            ><span class="sr-only">{{ t.kind === "income" ? "Income" : "Expense" }}</span>{{ formatAmount(t) }}</td>
          </tr>
          <tr v-if="hasTrail(t)" class="border-b border-rule">
            <td></td>
            <td :id="`trail-${t.id}`" colspan="4" class="pb-2 pr-0">
              <TransactionTrail :tx="t" :categories="categories" @goto="emit('goto', $event)" />
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>
