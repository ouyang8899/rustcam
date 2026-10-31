<script setup>
const props = defineProps({
  name: { type: String, required: true },
  size: { type: Number, default: 20 }
})

const ICONS = {
  camera: ['M4 8h3l1.5-2h7L17 8h3a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z', 'M12 11.5a3 3 0 1 1 0 6 3 3 0 0 1 0-6z'],
  server: ['M4 4h16v6H4z', 'M4 14h16v6H4z', 'M7 7h.01', 'M7 17h.01'],
  'map-pin': ['M12 21s7-5.6 7-11a7 7 0 1 0-14 0c0 5.4 7 11 7 11z', 'M12 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z'],
  broadcast: ['M12 13.5a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z', 'M8.5 16a5 5 0 0 1 0-7', 'M5 19a9 9 0 0 1 0-13'],
  shield: ['M12 3l7.5 3v5.5c0 4.6-3.1 8.4-7.5 9.5-4.4-1.1-7.5-4.9-7.5-9.5V6L12 3z', 'M9 12l2.2 2.2L15.5 10'],
  cpu: ['M7.5 7.5h9v9h-9z', 'M10 3v4.5', 'M14 3v4.5', 'M10 16.5V21', 'M14 16.5V21', 'M3 10h4.5', 'M3 14h4.5', 'M16.5 10H21', 'M16.5 14H21'],
  layers: ['M12 3l9 5-9 5-9-5 9-5z', 'M3 13l9 5 9-5'],
  chart: ['M4 20V11', 'M10 20V4', 'M16 20v-6', 'M21 20H3'],
  zap: ['M13 2L4 14h6l-1 8 9-12h-6l1-8z'],
  code: ['M9 8l-5 4 5 4', 'M15 8l5 4-5 4'],
  globe: ['M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z', 'M3 12h18', 'M12 3c2.5 2.6 3.8 5.7 3.8 9S14.5 18.4 12 21c-2.5-2.6-3.8-5.7-3.8-9S9.5 5.6 12 3z'],
  lock: ['M6 11h12v9H6z', 'M9 11V8a3 3 0 0 1 6 0v3'],
  users: ['M9.5 11a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z', 'M2.5 20a7 7 0 0 1 14 0', 'M16.5 11.5a3 3 0 1 0 0-6', 'M17.5 20h4a5.5 5.5 0 0 0-4.2-5.3'],
  clock: ['M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z', 'M12 7v5l3.5 2'],
  bell: ['M6 9.5a6 6 0 1 1 12 0c0 5 2 6.5 2 6.5H4s2-1.5 2-6.5z', 'M10 19.5a2 2 0 0 0 4 0'],
  grid: ['M4 4h7v7H4z', 'M13 4h7v7h-7z', 'M4 13h7v7H4z', 'M13 13h7v7h-7z'],
  file: ['M14 3H7a1 1 0 0 0-1 1v16a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V7l-4-4z', 'M14 3v4h4', 'M9 13h6', 'M9 17h4'],
  terminal: ['M4 5h16v14H4z', 'M8 10l2.5 2L8 14', 'M13 15h4'],
  play: ['M8 5.5v13l11-6.5-11-6.5z'],
  route: ['M6 19a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z', 'M18 10a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z', 'M15.5 7.5H10a3 3 0 0 0 0 6h4a3 3 0 0 1 0 6H8.5']
}

const paths = computed(() => ICONS[props.name] ?? [])
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.6"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <path v-for="(d, i) in paths" :key="i" :d="d" />
  </svg>
</template>
