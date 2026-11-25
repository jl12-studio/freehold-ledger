<script setup lang="ts">
import { computed, ref } from "vue";
import { formatCents } from "../lib/api";
import type { Transaction } from "../lib/api";
import { MONTHS, monthlyTotals, totals } from "../lib/ledgerMath";

const props = withDefaults(
  defineProps<{ year: number; rows: Transaction[]; today?: Date; selectedMonth?: number | null }>(),
  { today: () => new Date(), selectedMonth: null },
);
/** A month (0-11) was clicked: the window shows that month's entries, or clears it when it was already shown. */
const emit = defineEmits<{ month: [month: number] }>();

const sum = computed(() => totals(props.rows));
const net = computed(() => sum.value.in_cents - sum.value.out_cents);
const netParts = computed(() => {
  const s = formatCents(net.value);
  const dot = s.lastIndexOf(".");
  return { whole: s.slice(0, dot), cents: s.slice(dot) };
});
const isThisYear = computed(() => props.year === props.today.getFullYear());
/** Months after this one have not happened yet; only meaningful for the current year. */
const nowMonth = computed(() => (isThisYear.value ? props.today.getMonth() : props.year < props.today.getFullYear() ? 11 : -1));
const months = computed(() => monthlyTotals(props.rows, props.year));
/** A month after today with nothing in it; a future-dated entry keeps its month live. */
const isEmptyFuture = (i: number) => i > nowMonth.value && !months.value[i].in_cents && !months.value[i].out_cents;

// Chart geometry (viewBox units). One scale for both directions, so bar heights compare honestly.
const W = 600;
const COL = W / 12;
const HALF = 44;
const MID = HALF + 2;
const BAR = 22;
const peak = computed(() => Math.max(1, ...months.value.map((m) => Math.max(m.in_cents, m.out_cents))));
const h = (cents: number) => (cents > 0 ? Math.max(1.5, (cents / peak.value) * HALF) : 0);

const hover = ref<number | null>(null);
const readout = computed(() => {
  const i = hover.value;
  if (i === null) return null;
  if (isEmptyFuture(i)) return { month: MONTHS[i], text: "nothing yet" };
  const m = months.value[i];
  if (!m.in_cents && !m.out_cents) return { month: MONTHS[i], text: "no entries" };
  return { month: MONTHS[i], in: formatCents(m.in_cents), out: formatCents(m.out_cents) };
});
function monthLabel(i: number): string {
  const m = months.value[i];
  const totalsText = m.in_cents || m.out_cents
    ? `received ${formatCents(m.in_cents)}, paid out ${formatCents(m.out_cents)}`
    : "no entries";
  return `${MONTHS[i]} ${props.year}: ${totalsText}`;
}
function pick(i: number) {
  if (!isEmptyFuture(i)) emit("month", i);
}
</script>

<template>
  <section class="year-band" :aria-label="`Tax year ${year}`">
    <div>
      <p class="eyebrow">Net for {{ year }}<template v-if="isThisYear">, so far</template></p>
      <p class="figure mt-2" data-testid="net">
        {{ netParts.whole }}<span class="figure-cents">{{ netParts.cents }}</span>
      </p>
      <p class="mt-3.5 flex flex-wrap gap-x-5 gap-y-1 text-ui text-muted tabular-nums">
        <span>Received <b class="font-[560] text-income" data-testid="received">{{ formatCents(sum.in_cents) }}</b></span>
        <span>Paid out <b class="font-[560] text-fg" data-testid="paid-out">{{ formatCents(sum.out_cents) }}</b></span>
      </p>
    </div>

    <div class="min-w-0">
      <div class="mb-2 flex items-baseline justify-between gap-3">
        <p class="eyebrow">Month by month</p>
        <p class="text-right text-caption text-muted tabular-nums" aria-hidden="true">
          <template v-if="readout && 'in' in readout">
            {{ readout.month }} · received <b class="font-[560] text-fg">{{ readout.in }}</b> · paid out <b class="font-[560] text-fg">{{ readout.out }}</b>
          </template>
          <template v-else-if="readout">{{ readout.month }}: {{ readout.text }}</template>
          <template v-else>Click a month to see its entries</template>
        </p>
      </div>
      <svg
        class="month-chart"
        :viewBox="`0 0 ${W} ${MID + HALF + 28}`"
        role="group"
        :aria-label="`Months of ${year}`"
        @mouseleave="hover = null"
      >
        <line class="chart-axis" x1="0" :x2="W" :y1="MID" :y2="MID" aria-hidden="true" />
        <g
          v-for="(m, i) in months"
          :key="i"
          :class="['chart-col', { 'is-future': isEmptyFuture(i), 'is-on': hover === i, 'is-selected': selectedMonth === i }]"
          :role="isEmptyFuture(i) ? undefined : 'button'"
          :tabindex="isEmptyFuture(i) ? undefined : 0"
          :aria-label="isEmptyFuture(i) ? undefined : monthLabel(i)"
          :aria-pressed="isEmptyFuture(i) ? undefined : selectedMonth === i"
          :aria-hidden="isEmptyFuture(i) ? 'true' : undefined"
          @mouseenter="hover = i"
          @focus="hover = i"
          @blur="hover = null"
          @click="pick(i)"
          @keydown.enter.prevent="pick(i)"
          @keydown.space.prevent="pick(i)"
        >
          <rect class="chart-hit" :x="i * COL + 2" y="0" :width="COL - 4" :height="MID + HALF + 4" rx="3" />
          <rect v-if="m.in_cents" class="chart-in" :x="i * COL + (COL - BAR) / 2" :y="MID - h(m.in_cents)" :width="BAR" :height="h(m.in_cents)" rx="1.5" />
          <rect v-if="m.out_cents" class="chart-out" :x="i * COL + (COL - BAR) / 2" :y="MID + 1" :width="BAR" :height="h(m.out_cents)" rx="1.5" />
          <text :class="['chart-label', { 'is-now': isThisYear && i === nowMonth }]" :x="i * COL + COL / 2" :y="MID + HALF + 22" text-anchor="middle">{{ MONTHS[i].slice(0, 3) }}</text>
        </g>
      </svg>
    </div>
  </section>
</template>
