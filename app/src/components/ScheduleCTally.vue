<script setup lang="ts">
import { computed } from "vue";
import { formatCents } from "../lib/api";
import type { Transaction } from "../lib/api";
import { scheduleCTotals, totals } from "../lib/ledgerMath";

const props = defineProps<{ year: number; rows: Transaction[] }>();

const lines = computed(() => scheduleCTotals(props.rows));
const expenses = computed(() => totals(props.rows).out_cents);
const entries = (n: number) => `${n} ${n === 1 ? "entry" : "entries"}`;
</script>

<template>
  <aside class="tally" aria-labelledby="tally-title">
    <h2 id="tally-title" class="font-serif text-[1.1875rem] leading-tight font-semibold tracking-[-0.01em]">Schedule C, {{ year }}</h2>
    <p class="mt-1 text-caption text-muted">What this year's entries add up to, line by line.</p>
    <ol v-if="lines.length > 0" class="mt-3.5">
      <li v-for="l in lines" :key="`${l.kind}:${l.line ?? l.label}`" class="tally-row" :data-kind="l.kind">
        <span class="font-mono text-[0.71875rem] text-muted">
          <template v-if="l.line"><span class="sr-only">Line </span><span aria-hidden="true">L</span>{{ l.line }}</template>
          <span v-else aria-hidden="true">·</span>
        </span>
        <span class="min-w-0">
          {{ l.label }}
          <small class="block text-[0.75rem] text-muted">{{ entries(l.count) }}<template v-if="l.assets"> · {{ l.assets }} marked as asset</template></small>
        </span>
        <span :class="['font-[560] tabular-nums', l.kind === 'income' ? 'text-income' : '']">{{ formatCents(l.total_cents) }}</span>
      </li>
    </ol>
    <p v-else class="mt-3.5 font-serif text-ui italic text-muted">Nothing recorded in {{ year }} yet.</p>
    <div class="tally-total">
      <span>Total expenses</span>
      <span class="font-serif text-[1.125rem] tabular-nums" data-testid="tally-total">{{ formatCents(expenses) }}</span>
    </div>
    <p class="mt-3 text-[0.78125rem] text-muted">Voided entries are left out. Export {{ year }} for the full worksheet.</p>
  </aside>
</template>
