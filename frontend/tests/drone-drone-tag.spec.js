import { test, expect, loginAs } from './fixtures.js'

test('无人机列表以 drone_tag 作为标识', async ({ page }) => {
  await loginAs(page)
  await page.goto('/drone/drones')
  await expect(page.locator('[data-testid="drone-tag"]').first()).toHaveText(/^drone-/)
})
