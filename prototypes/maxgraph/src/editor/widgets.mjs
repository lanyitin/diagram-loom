/**
 * 面板上的小零件。
 *
 * # 為什麼自己寫，不用元件庫
 *
 * 原型不該為了幾個輸入框裝一個 UI 框架。而且**這一層將來會被 Vue 重寫**——
 * 這裡要驗的是「需要哪些控制項、它們各自要接哪個 maxGraph API」，
 * 不是「這幾個框長什麼樣」。
 *
 * # 一條規矩：控制項不自己記狀態
 *
 * 每個零件都有 `set(value)`，由面板在選取改變時**整批灌值**。
 * 控制項自己記狀態的話，圖上被別的路徑改掉（復原、對齊、程式產生）時
 * 面板會停在舊值——而使用者會相信面板寫的那個。
 */

/** 一列：左邊標籤、右邊控制項。 */
export function row(label, ...controls) {
  const el = document.createElement('label')
  el.className = 'row'
  const name = document.createElement('span')
  name.className = 'label'
  name.textContent = label
  el.append(name, ...controls)
  return el
}

export function group(title, ...children) {
  const el = document.createElement('section')
  el.className = 'group'
  const head = document.createElement('h2')
  head.textContent = title
  el.append(head, ...children)
  return el
}

/**
 * 顏色。
 *
 * 附一個「無」的核取方塊：`fillColor=none` 跟「黑色」是兩件事，而原生的
 * `<input type=color>` 表達不了「沒有顏色」——它永遠是某個顏色。
 * 我們的容器框正是 `fillColor: none`，少了這個就沒辦法把填色改回透明。
 */
export function color(onChange, { none = true } = {}) {
  const input = document.createElement('input')
  input.type = 'color'
  input.className = 'color'
  const clear = document.createElement('input')
  clear.type = 'checkbox'
  clear.className = 'none'
  clear.title = '無'

  input.oninput = () => { clear.checked = false; onChange(input.value) }
  clear.onchange = () => onChange(clear.checked ? 'none' : input.value)

  const wrap = document.createElement('span')
  wrap.className = 'colorwrap'
  wrap.append(input)
  if (none) wrap.append(clear)

  wrap.set = (value) => {
    const isNone = !value || value === 'none'
    clear.checked = isNone
    // `<input type=color>` 只吃 #rrggbb。給它別的（none、rgb()、名字）
    // 會被瀏覽器安靜地換成 #000000，看起來像「這個形狀是黑的」。
    if (!isNone && /^#[0-9a-f]{6}$/i.test(value)) input.value = value
  }
  return wrap
}

export function number(onChange, { min = 0, max = 9999, step = 1, width = 56 } = {}) {
  const input = document.createElement('input')
  input.type = 'number'
  input.className = 'num'
  Object.assign(input, { min, max, step })
  input.style.width = `${width}px`
  input.oninput = () => onChange(Number(input.value))
  input.set = (value) => { input.value = value ?? '' }
  return input
}

export function toggle(label, onChange, title = '') {
  const button = document.createElement('button')
  button.className = 'toggle'
  button.textContent = label
  button.title = title || label
  button.onclick = () => onChange(button.getAttribute('aria-pressed') !== 'true')
  button.set = (on) => button.setAttribute('aria-pressed', String(Boolean(on)))
  button.set(false)
  return button
}

export function select(options, onChange) {
  const el = document.createElement('select')
  for (const [value, label] of options) {
    const option = document.createElement('option')
    option.value = value
    option.textContent = label
    el.append(option)
  }
  el.onchange = () => onChange(el.value)
  el.set = (value) => { el.value = value ?? '' }
  return el
}

/** 一排按鈕（對齊、置前置後這種一次一個動作的）。 */
export function actions(items) {
  const el = document.createElement('div')
  el.className = 'actions'
  for (const [label, run, title] of items) {
    const button = document.createElement('button')
    button.textContent = label
    button.title = title ?? label
    button.dataset.action = title ?? label
    button.onclick = run
    el.append(button)
  }
  return el
}
