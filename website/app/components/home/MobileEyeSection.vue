<script setup>
defineProps({
  data: { type: Object, required: true }
})

const localePath = useLocalePath()

const coneRef = ref(null)
const targetRef = ref(null)
const trailRef = ref(null)

const heading = ref(0)
const altitude = ref(0)
const battery = ref(0)
const distance = ref(0)

const ORIGIN = { x: 240, y: 296 }
const CONE_LENGTH = 250
const CONE_HALF_ANGLE = 23
const TRAIL_POINTS = 40
const TRAIL_SAMPLE = 6

let handle = 0

const conePath = computed(() => {
  const rad = (CONE_HALF_ANGLE * Math.PI) / 180
  const dx = CONE_LENGTH * Math.sin(rad)
  const dy = -CONE_LENGTH * Math.cos(rad)
  const left = { x: ORIGIN.x - dx, y: ORIGIN.y + dy }
  const right = { x: ORIGIN.x + dx, y: ORIGIN.y + dy }
  return `M ${ORIGIN.x} ${ORIGIN.y} L ${left.x.toFixed(1)} ${left.y.toFixed(1)} A ${CONE_LENGTH} ${CONE_LENGTH} 0 0 1 ${right.x.toFixed(1)} ${right.y.toFixed(1)} Z`
})

const edgeLeft = computed(() => {
  const rad = (CONE_HALF_ANGLE * Math.PI) / 180
  return { x: ORIGIN.x - CONE_LENGTH * Math.sin(rad), y: ORIGIN.y - CONE_LENGTH * Math.cos(rad) }
})

const edgeRight = computed(() => {
  const rad = (CONE_HALF_ANGLE * Math.PI) / 180
  return { x: ORIGIN.x + CONE_LENGTH * Math.sin(rad), y: ORIGIN.y - CONE_LENGTH * Math.cos(rad) }
})

function targetPosition(t) {
  return {
    x: ORIGIN.x + Math.sin(t * 0.52) * 152,
    y: 118 + Math.sin(t * 0.31 + 0.9) * 52
  }
}

onMounted(() => {
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
  const start = performance.now()
  const trail = []
  let frame = 0

  const paint = (t) => {
    const pos = targetPosition(t)
    const bearing = (Math.atan2(pos.y - ORIGIN.y, pos.x - ORIGIN.x) * 180) / Math.PI
    const rotation = bearing + 90

    if (coneRef.value) {
      coneRef.value.setAttribute('transform', `rotate(${rotation.toFixed(2)} ${ORIGIN.x} ${ORIGIN.y})`)
    }
    if (targetRef.value) {
      targetRef.value.setAttribute('transform', `translate(${pos.x.toFixed(1)} ${pos.y.toFixed(1)})`)
    }

    if (frame % TRAIL_SAMPLE === 0) {
      trail.push({ x: pos.x, y: pos.y })
      if (trail.length > TRAIL_POINTS) trail.shift()
      if (trailRef.value) {
        trailRef.value.setAttribute(
          'points',
          trail.map((p) => `${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ')
        )
      }
    }

    heading.value = Math.round((rotation + 360) % 360)
    altitude.value = Math.round(38 + Math.sin(t * 0.42) * 24)
    battery.value = Math.round(96 - ((t * 1.4) % 70))
    distance.value = (Math.hypot(pos.x - ORIGIN.x, pos.y - ORIGIN.y) / 8).toFixed(1)

    frame += 1
  }

  if (reduced) {
    paint(0)
    return
  }

  const loop = (now) => {
    paint((now - start) / 1000)
    handle = requestAnimationFrame(loop)
  }
  handle = requestAnimationFrame(loop)
})

onBeforeUnmount(() => cancelAnimationFrame(handle))
</script>

<template>
  <section class="relative overflow-hidden border-y border-white/5">
    <div class="bg-grid absolute inset-0 opacity-70" />
    <div
      class="pointer-events-none absolute -top-40 left-1/2 h-[420px] w-[820px] -translate-x-1/2 rounded-full opacity-25 blur-[120px]"
      style="background: radial-gradient(circle, var(--color-accent) 0%, transparent 62%)"
    />

    <div class="relative mx-auto max-w-6xl px-6 py-24 sm:py-28">
      <div class="grid items-center gap-16 lg:grid-cols-[1fr_1fr]">
        <div>
          <p class="text-accent text-xs font-semibold tracking-[0.24em] uppercase">
            {{ data.eyebrow }}
          </p>
          <h2 class="mt-5 text-3xl leading-tight font-semibold tracking-tight text-white sm:text-4xl">
            {{ data.title }}
          </h2>
          <p class="text-muted mt-5 max-w-xl text-base leading-relaxed">
            {{ data.description }}
          </p>

          <div class="mt-9 grid gap-4 sm:grid-cols-2">
            <UiReveal v-for="(principle, i) in data.principles" :key="principle.title" :delay="i * 80">
              <div class="panel h-full p-5">
                <h3 class="text-accent font-mono text-sm font-semibold">{{ principle.title }}</h3>
                <p class="text-muted mt-2 text-sm leading-relaxed">{{ principle.desc }}</p>
              </div>
            </UiReveal>
          </div>

          <ul class="mt-8 flex flex-wrap gap-2">
            <li
              v-for="capability in data.capabilities"
              :key="capability"
              class="rounded-full border border-white/10 bg-ink-900/60 px-3 py-1.5 font-mono text-xs text-muted"
            >
              {{ capability }}
            </li>
          </ul>

          <div class="mt-9 flex flex-wrap items-center gap-4">
            <NuxtLink
              :to="localePath('/docs')"
              class="rounded-xl bg-accent px-5 py-3 text-sm font-medium text-ink-950 transition-colors hover:bg-accent/85"
            >
              {{ data.cta }}
            </NuxtLink>
            <span class="text-muted inline-flex items-center gap-2 text-xs">
              <span class="inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-accent-amber" />
              {{ data.status }}
            </span>
          </div>
        </div>

        <div class="relative">
          <div class="panel overflow-hidden">
            <div class="flex items-center justify-between border-b border-white/5 px-4 py-3">
              <div class="flex items-center gap-2">
                <span class="h-1.5 w-1.5 rounded-full bg-accent" />
                <span class="font-mono text-xs text-white">{{ data.hud.title }}</span>
              </div>
              <span class="font-mono text-xs text-muted">{{ data.hud.link }}</span>
            </div>

            <svg viewBox="0 0 480 320" class="block w-full" role="img" :aria-label="data.hud.alt">
              <defs>
                <linearGradient id="cone-fill" x1="0" y1="1" x2="0" y2="0">
                  <stop offset="0%" stop-color="var(--color-accent)" stop-opacity="0.55" />
                  <stop offset="55%" stop-color="var(--color-accent)" stop-opacity="0.22" />
                  <stop offset="100%" stop-color="var(--color-accent)" stop-opacity="0" />
                </linearGradient>
                <radialGradient id="target-glow">
                  <stop offset="0%" stop-color="var(--color-accent)" stop-opacity="0.9" />
                  <stop offset="100%" stop-color="var(--color-accent)" stop-opacity="0" />
                </radialGradient>
              </defs>

              <g stroke="rgba(148,163,184,0.09)" stroke-width="1">
                <line v-for="x in [40, 120, 200, 280, 360, 440]" :key="`x${x}`" :x1="x" y1="0" :x2="x" y2="320" />
                <line v-for="y in [40, 100, 160, 220, 280]" :key="`y${y}`" x1="0" :y1="y" x2="480" :y2="y" />
              </g>

              <g fill="none" stroke="rgba(34,211,238,0.16)" stroke-width="1">
                <path d="M 240 296 m -90 0 a 90 90 0 0 1 180 0" />
                <path d="M 240 296 m -160 0 a 160 160 0 0 1 320 0" />
                <path d="M 240 296 m -230 0 a 230 230 0 0 1 460 0" />
              </g>

              <polyline
                ref="trailRef"
                points=""
                fill="none"
                stroke="var(--color-accent)"
                stroke-opacity="0.55"
                stroke-width="1.75"
                stroke-linecap="round"
              />

              <g ref="coneRef">
                <path :d="conePath" fill="url(#cone-fill)" />
                <line
                  :x1="ORIGIN.x"
                  :y1="ORIGIN.y"
                  :x2="edgeLeft.x"
                  :y2="edgeLeft.y"
                  stroke="var(--color-accent)"
                  stroke-opacity="0.55"
                  stroke-width="1"
                />
                <line
                  :x1="ORIGIN.x"
                  :y1="ORIGIN.y"
                  :x2="edgeRight.x"
                  :y2="edgeRight.y"
                  stroke="var(--color-accent)"
                  stroke-opacity="0.55"
                  stroke-width="1"
                />
              </g>

              <g ref="targetRef">
                <circle r="26" fill="url(#target-glow)" />
                <circle r="5.5" fill="var(--color-accent)" />
                <circle r="11" fill="none" stroke="var(--color-accent)" stroke-opacity="0.65" stroke-width="1" />
              </g>

              <g transform="translate(240 296)">
                <circle r="9" fill="var(--color-ink-950)" stroke="var(--color-accent)" stroke-width="1.5" />
                <circle r="2.6" fill="var(--color-accent)" />
              </g>
            </svg>

            <dl class="grid grid-cols-4 gap-px border-t border-white/5 bg-white/5 text-center">
              <div v-for="readout in data.hud.readouts" :key="readout.label" class="bg-ink-900/90 px-2 py-3">
                <dt class="text-muted font-mono text-[10px] tracking-widest uppercase">{{ readout.label }}</dt>
                <dd class="mt-1 font-mono text-sm text-white">
                  <span v-if="readout.key === 'heading'">{{ String(heading).padStart(3, '0') }}°</span>
                  <span v-else-if="readout.key === 'altitude'">{{ altitude }} m</span>
                  <span v-else-if="readout.key === 'battery'">{{ battery }}%</span>
                  <span v-else>{{ distance }} m</span>
                </dd>
              </div>
            </dl>
          </div>

          <p class="text-muted mt-4 text-center font-mono text-xs">
            {{ data.hud.caption }}
          </p>
        </div>
      </div>
    </div>
  </section>
</template>
