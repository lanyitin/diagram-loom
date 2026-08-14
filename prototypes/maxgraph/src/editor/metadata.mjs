/**
 * 編輯資料（draw.io 的 Edit Data，⌘M）。
 *
 * # 為什麼這個功能對這個專案特別重要
 *
 * draw.io 的自訂屬性就是**我們的 `loomId` 住的地方**。所以這不只是一個
 * 「進階使用者才用得到」的對話框——它是綁定機制的手動入口，
 * 也是使用者看得到「這個形狀到底代表誰」的唯一地方。
 *
 * # ⚠️ 值從字串換成 XML 元素的那一刻
 *
 * maxGraph 的 `cell.value` 平常是字串。要放自訂屬性就得換成一個 XML 元素
 * （`<object label="..." loomId="...">`）——draw.io 按「編輯資料」時做的
 * 也是這件事，我們正式版的 `bind()` 也是。
 *
 * 換過去之後有一個**一定會踩到的坑**：`convertValueToString` 的預設實作
 * 遇到節點會回 `nodeName`，於是圖上每個形狀的字都變成 `object`。
 * 所以換值之前一定要先把那個方法換掉，見 [`useElementValues`]。
 */

/** 這幾個鍵是機器在用的，畫面上要標出來，但**不禁止編輯**——這是逃生口。 */
const RESERVED = ['loomId', 'loomKind']

/**
 * 讓圖看得懂「值是一個 XML 元素」。
 *
 * 少了這一段，第一次按下「編輯資料」之後，那個形狀的標籤會變成 `object`。
 * 它不會報錯，只是圖上多一個看不懂的字。
 */
export function useElementValues(graph) {
  graph.convertValueToString = (cell) => {
    const value = cell.getValue()
    if (value?.nodeType === 1) return value.getAttribute('label') ?? ''
    return value == null ? '' : String(value)
  }

  // 就地編輯（雙擊改字）也要跟著：改的是 `label` 這個屬性，不是整個值。
  const setValue = graph.labelChanged?.bind(graph)
  graph.labelChanged = (cell, label, evt) => {
    const value = cell.getValue()
    if (value?.nodeType === 1) {
      const next = value.cloneNode(true)
      next.setAttribute('label', label)
      graph.model.setValue(cell, next)
      return cell
    }
    return setValue(cell, label, evt)
  }
}

/** 讀一個自訂屬性。值還是字串（沒有屬性）時回 `null`。 */
export function attribute(cell, name) {
  const value = cell?.getValue()
  return value?.nodeType === 1 ? value.getAttribute(name) : null
}

/**
 * 寫一個自訂屬性。傳 `null` 就是拿掉。
 *
 * 綁定要寫在**形狀上**，不是記在畫面的某個變數裡。記在變數裡的話，圖存出去
 * 什麼都沒有、對帳看不見，而畫面上卻一副「已經指定好了」的樣子——
 * 那是這個工具最不能出的那種錯。
 */
export function setAttribute(graph, cell, name, value) {
  const next = asElement(cell)
  if (value == null) next.removeAttribute(name)
  else next.setAttribute(name, value)
  graph.batchUpdate(() => graph.model.setValue(cell, next))
}

/** 把 cell 的值換成 XML 元素（如果它還是字串）。回傳那個元素的複本。 */
function asElement(cell) {
  const value = cell.getValue()
  if (value?.nodeType === 1) return value.cloneNode(true)

  const doc = new DOMParser().parseFromString('<object/>', 'text/xml')
  const el = doc.documentElement
  el.setAttribute('label', value == null ? '' : String(value))
  return el
}

/**
 * 開一個「編輯資料」對話框。
 *
 * 一次只編一個 cell——多選時改屬性是什麼意思沒有標準答案，draw.io 也是
 * 只給單選。
 */
export function editMetadata(graph, cell, { onChange = () => {} } = {}) {
  const element = asElement(cell)

  const back = document.createElement('div')
  back.className = 'modal'
  const box = document.createElement('div')
  box.className = 'dialog'
  back.append(box)

  const title = document.createElement('h2')
  title.textContent = '編輯資料'
  const rows = document.createElement('div')
  rows.className = 'rows'
  box.append(title, rows)

  /** 畫出所有屬性。`label` 也列出來——它就是圖上的那行字，不該藏起來。 */
  function draw() {
    rows.textContent = ''
    for (const name of element.getAttributeNames()) {
      rows.append(attributeRow(name))
    }
  }

  function attributeRow(name) {
    const row = document.createElement('div')
    row.className = 'metarow'

    const key = document.createElement('input')
    key.value = name
    key.className = 'key'
    key.readOnly = RESERVED.includes(name)
    key.title = RESERVED.includes(name) ? '這個鍵是綁定用的，改名會讓對帳看不見它' : ''
    key.onchange = () => {
      const next = key.value.trim()
      if (!next || next === name) { key.value = name; return }
      const value = element.getAttribute(name)
      element.removeAttribute(name)
      element.setAttribute(next, value)
      draw()
    }

    const value = document.createElement('input')
    value.value = element.getAttribute(name) ?? ''
    value.className = 'value'
    value.oninput = () => element.setAttribute(key.value, value.value)

    const remove = document.createElement('button')
    remove.textContent = '✕'
    remove.title = '刪掉這一項'
    remove.className = 'x'
    remove.onclick = () => { element.removeAttribute(name); draw() }

    row.append(key, value, remove)
    if (RESERVED.includes(name)) row.classList.add('reserved')
    return row
  }

  const add = document.createElement('button')
  add.textContent = '＋ 新增一項'
  add.className = 'add'
  add.onclick = () => {
    let name = '屬性'
    let n = 1
    while (element.hasAttribute(name)) name = `屬性${n += 1}`
    element.setAttribute(name, '')
    draw()
  }

  const foot = document.createElement('div')
  foot.className = 'foot'
  const cancel = document.createElement('button')
  cancel.textContent = '取消'
  cancel.onclick = () => back.remove()
  const ok = document.createElement('button')
  ok.textContent = '套用'
  ok.className = 'primary'
  ok.onclick = () => {
    graph.batchUpdate(() => graph.model.setValue(cell, element))
    back.remove()
    onChange()
  }
  foot.append(cancel, ok)
  box.append(add, foot)

  // 點外面關掉、Esc 關掉。對話框沒有這兩個會讓人覺得被困住。
  back.onclick = (e) => { if (e.target === back) back.remove() }
  back.onkeydown = (e) => { if (e.key === 'Escape') back.remove() }

  draw()
  document.body.append(back)
  box.querySelector('input')?.focus()
  return back
}
