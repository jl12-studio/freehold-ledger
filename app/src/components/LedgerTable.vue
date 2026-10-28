<script setup lang="ts">
import { formatCents, formatDate, paymentCaption } from "../lib/api";
import type { Category, Transaction } from "../lib/api";
import TransactionTrail from "./TransactionTrail.vue";

defineProps<{ rows: Transaction[]; categories: Category[] }>();
const emit = defineEmits<{ goto: [id: number] }>();

function hasTrail(t: Transaction): boolean {
  return !!t.void || t.replaced_by_id !== null || t.replaces_id !== null || t.classification_history.length > 0;
}
</script>

<template>
  <div class="overflow-x-auto">
    <table class="w-full min-w-[52rem] border-collapse text-left text-ui">
      <caption class="sr-only">Ledger entries</caption>
      <thead>
        <tr>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Date</th>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Description</th>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Category</th>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Payment method</th>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Source</th>
          <th scope="col" class="border-b border-rule py-2 pr-4 font-sans text-caption font-[560] text-muted">Kind</th>
          <th scope="col" class="border-b border-rule py-2 text-right font-sans text-caption font-[560] text-muted">Amount</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="t in rows" :key="t.id">
          <tr
            :id="`tx-${t.id}`"
            tabindex="-1"
            :aria-describedby="hasTrail(t) ? `trail-${t.id}` : undefined"
            :class="['align-top', hasTrail(t) ? '' : 'border-b border-rule']"
          >
            <td class="whitespace-nowrap py-3 pr-4">{{ formatDate(t.occurred_on) }}</td>
            <td :class="['py-3 pr-4', t.void ? 'line-through text-muted' : '']">
              {{ t.description }}
              <span v-if="t.is_asset" class="ml-1 text-caption text-muted no-underline">Asset</span>
            </td>
            <td class="py-3 pr-4">
              <template v-if="t.category === 'uncategorized'">Needs a category</template>
              <template v-else>{{ t.category_label }}</template>
              <span v-if="t.schedule_c_line" class="block text-caption text-muted">Sch. C line {{ t.schedule_c_line }}</span>
            </td>
            <td class="py-3 pr-4">
              <template v-if="t.payment_method">
                {{ t.payment_method.nickname }}
                <span class="block text-caption text-muted">{{ paymentCaption(t.payment_method) }}</span>
              </template>
              <span v-else class="text-muted">Not set</span>
            </td>
            <td class="py-3 pr-4">{{ t.income_source ?? "—" }}</td>
            <td class="py-3 pr-4">{{ t.kind === "income" ? "Income" : "Expense" }}</td>
            <td :class="['whitespace-nowrap py-3 text-right tabular-nums', t.void ? 'line-through text-muted' : '']">{{ formatCents(t.amount_cents) }}</td>
          </tr>
          <tr v-if="hasTrail(t)">
            <td :id="`trail-${t.id}`" colspan="7" class="border-b border-rule pb-3">
              <TransactionTrail :tx="t" :categories="categories" @goto="emit('goto', $event)" />
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>
