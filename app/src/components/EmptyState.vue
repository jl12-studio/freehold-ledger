<script setup lang="ts">
defineProps<{ businessName: string; currentName: string | null; isCurrent: boolean }>();
defineEmits<{ reveal: []; connect: [] }>();
</script>

<template>
  <section class="max-w-measure mx-auto px-6 pt-16">
    <h2 class="font-serif text-h2 font-semibold">Nothing recorded in {{ businessName }} yet</h2>
    <p v-if="isCurrent" class="mt-4 font-serif text-body">
      Tell your agent what you bought or earned and it lands here.
    </p>
    <p v-else-if="currentName" class="mt-4 font-serif text-body">
      Your agent records in {{ currentName }}. To record here, tell it: “switch to {{ businessName }}”.
    </p>
    <p v-else class="mt-4 font-serif text-body">
      Tell your agent which business to record in, for example: “switch to {{ businessName }}”.
    </p>
    <p class="mt-3 text-ui text-muted">
      This file is yours: copy it, back it up, open it with any SQLite tool.
    </p>
    <div class="mt-6 flex flex-wrap items-center gap-x-6 gap-y-2">
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('connect')"
      >
        Connect your agent
      </button>
      <button
        type="button"
        class="min-h-8 cursor-pointer text-ui font-[560] underline decoration-accent underline-offset-4 hover:text-accent"
        @click="$emit('reveal')"
      >
        Show the ledger file
      </button>
    </div>
  </section>
</template>
