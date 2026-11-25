<script setup lang="ts">
import { computed, onBeforeUnmount } from "vue";
import { formatCents, formatDate } from "../lib/api";
import type { Category, Transaction } from "../lib/api";
import { MONTHS, dateParts, groupByMonth } from "../lib/ledgerMath";
import TransactionTrail from "./TransactionTrail.vue";

const props = defineProps<{ rows: Transaction[]; categories: Category[] }>();
const emit = defineEmits<{ goto: [id: number] }>();

const groups = computed(() => groupByMonth(props.rows));
/** Position of each row in the whole list, for the staggered entrance (capped so long lists do not drag). */
const order = computed(() => new Map(props.rows.map((r, i) => [r.id, Math.min(i, 16)])));

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

const th = "eyebrow pt-3.5 pb-2 pr-3 text-left";
</script>

<template>
  <div class="overflow-x-auto">
    <table class="ledger w-full min-w-[44rem] table-fixed border-collapse text-left font-sans text-ui">
      <caption class="sr-only">Ledger entries</caption>
      <colgroup>
        <col class="w-[4.25rem]" />
        <col />
        <col class="w-[11rem] lg:w-[13rem]" />
        <col class="w-[9.5rem] lg:w-[11rem]" />
        <col class="w-[6.75rem]" />
        <col class="w-[6.75rem]" />
      </colgroup>
      <thead>
        <tr>
          <th scope="col" :class="th">Day</th>
          <th scope="col" :class="th">Description</th>
          <th scope="col" :class="th">Category</th>
          <th scope="col" :class="th">Paid with</th>
          <th scope="col" :class="[th, 'amount-col text-right']">Paid out</th>
          <th scope="col" :class="[th, 'amount-col text-right']">Received</th>
        </tr>
      </thead>
      <tbody v-for="g in groups" :key="g.key" :data-month="g.key">
        <tr class="month-row" :style="{ '--i': order.get(g.rows[0].id) }">
          <th scope="rowgroup" colspan="6" class="pt-6 pb-2 text-left font-normal">
            <div class="flex items-baseline justify-between gap-3">
              <span class="font-serif text-[1.3125rem] font-[480] italic tracking-[-0.01em]">
                {{ MONTHS[g.month] }}<span class="ml-1.5 text-ui font-[420] not-italic text-muted tabular-nums">{{ g.year }}</span>
              </span>
              <span class="text-caption text-muted tabular-nums" data-testid="month-totals">
                <template v-if="g.totals.out_cents">out <b class="font-[560] text-fg">{{ formatCents(g.totals.out_cents) }}</b></template>
                <template v-if="g.totals.out_cents && g.totals.in_cents"> · </template>
                <template v-if="g.totals.in_cents">in <b class="font-[560] text-income">{{ formatCents(g.totals.in_cents) }}</b></template>
              </span>
            </div>
          </th>
        </tr>
        <template v-for="t in g.rows" :key="t.id">
          <tr
            :id="`tx-${t.id}`"
            tabindex="-1"
            :aria-describedby="hasTrail(t) ? `trail-${t.id}` : undefined"
            :class="['entry', hasTrail(t) ? 'has-trail' : '', t.void ? 'is-void' : '']"
            :style="{ '--i': order.get(t.id) }"
            @focus="flash"
          >
            <td class="py-2.5 pr-3 align-baseline tabular-nums">
              <span class="sr-only">{{ formatDate(t.occurred_on) }}</span>
              <span aria-hidden="true">
                <span class="block font-serif text-[1.1875rem] leading-none font-[520]">{{ dateParts(t.occurred_on).day }}</span>
                <span class="mt-0.5 block text-[0.6875rem] tracking-[0.06em] uppercase text-muted">{{ dateParts(t.occurred_on).weekday }}</span>
              </span>
            </td>
            <td class="py-2.5 pr-3 align-baseline">
              <div class="flex min-w-0 items-baseline gap-2">
                <span :class="['truncate font-[500]', t.void ? 'line-through text-muted' : '']" :title="t.description">{{ t.description }}</span>
                <span v-if="t.kind === 'income' && t.income_source" class="shrink-0 text-muted">from {{ t.income_source }}</span>
                <span v-if="t.is_asset" class="chip chip-asset shrink-0">Asset</span>
                <span v-if="t.void" class="chip chip-muted shrink-0">Voided</span>
                <span v-else-if="t.replaces_id !== null" class="chip chip-muted shrink-0">Correction</span>
              </div>
            </td>
            <td :class="['py-2.5 pr-3 align-baseline', t.void ? 'text-muted' : '']">
              <div class="flex min-w-0 items-baseline gap-2">
                <span v-if="t.category === 'uncategorized'" class="truncate font-[560] text-test">Needs a category</span>
                <template v-else>
                  <span class="truncate" :title="t.category_label">{{ t.category_label }}</span>
                  <span v-if="t.schedule_c_line" class="line-tag" :title="`Schedule C line ${t.schedule_c_line}`"><span class="sr-only">Schedule C line </span><span aria-hidden="true">L</span>{{ t.schedule_c_line }}</span>
                </template>
              </div>
            </td>
            <td :class="['truncate py-2.5 pr-3 align-baseline', t.void ? 'text-muted' : '']">
              <template v-if="t.payment_method">
                {{ t.payment_method.nickname }}
                <span v-if="t.payment_method.last4" class="ml-1 text-caption text-muted tabular-nums"><span class="sr-only">ending </span><span aria-hidden="true">··</span>{{ t.payment_method.last4 }}</span>
                <span v-if="t.payment_method.retired" class="chip chip-muted ml-1.5">Retired</span>
              </template>
              <span v-else class="font-serif italic text-muted">Not set</span>
            </td>
            <td
              :class="[
                'amount-col whitespace-nowrap py-2.5 text-right align-baseline tabular-nums font-[500]',
                t.void ? 'line-through text-muted' : '',
              ]"
            >
              <template v-if="t.kind === 'expense'"><span class="sr-only">Paid out </span>{{ formatCents(t.amount_cents) }}</template>
              <span v-else class="text-rule" aria-hidden="true">·</span>
            </td>
            <td
              :class="[
                'amount-col whitespace-nowrap py-2.5 text-right align-baseline tabular-nums font-[500]',
                t.void ? 'line-through text-muted' : 'text-income',
              ]"
            >
              <template v-if="t.kind === 'income'"><span class="sr-only">Received </span>{{ formatCents(t.amount_cents) }}</template>
              <span v-else class="text-rule" aria-hidden="true">·</span>
            </td>
          </tr>
          <tr v-if="hasTrail(t)" class="trail-row" :style="{ '--i': order.get(t.id) }">
            <td></td>
            <td :id="`trail-${t.id}`" colspan="5" class="pb-3 pr-0">
              <TransactionTrail :tx="t" :categories="categories" @goto="emit('goto', $event)" />
            </td>
          </tr>
        </template>
      </tbody>
    </table>
  </div>
</template>
