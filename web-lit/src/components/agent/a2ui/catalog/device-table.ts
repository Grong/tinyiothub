import { LitElement, html, css } from 'lit'
import { customElement, property } from 'lit/decorators.js'
import { hostStyles } from '../../../../styles/shared-host'

interface TableColumn {
  key: string
  label: string
}

interface TableRow {
  [key: string]: string | number | boolean
}

@customElement('device-table')
export class DeviceTable extends LitElement {
  @property({ type: Array }) columns: TableColumn[] = []
  @property({ type: Array }) rows: TableRow[] = []
  @property({ type: Number }) page = 1
  @property({ type: Number }) pageSize = 10
  @property({ type: Number }) totalCount = 0

  static styles = [hostStyles, css`
    :host { display: block; }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.8125rem;
    }
    th {
      text-align: left;
      padding: 6px 8px;
      box-shadow: 0 2px 0 0 var(--border);
      font-weight: 600;
      font-size: 0.75rem;
      color: var(--text-muted);
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }
    td {
      padding: 6px 8px;
      box-shadow: 0 1px 0 var(--card-highlight);
    }
    tbody tr { cursor: pointer; }
    tr:hover td {
      background: var(--bg-elevated);
    }
    .pagination {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-top: 8px;
      font-size: 0.75rem;
      color: var(--text-muted);
    }
    .pagination button {
      padding: 3px 8px;
      box-shadow: var(--glass-shadow-sm);
      border-radius: 4px;
      background: transparent;
      cursor: pointer;
      font-size: 0.75rem;
      color: var(--text);
    }
    .pagination button:disabled { opacity: 0.4; cursor: not-allowed; }
  `]

  private get totalPages(): number {
    return Math.ceil(this.totalCount / this.pageSize) || 1
  }

  private _handlePageChange(newPage: number) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'pageChange', page: newPage },
      bubbles: true, composed: true,
    }))
  }

  private _handleRowClick(row: TableRow, index: number) {
    this.dispatchEvent(new CustomEvent('a2ui-action', {
      detail: { action: 'rowClick', row, index },
      bubbles: true, composed: true,
    }))
  }

  render() {
    return html`
      <table>
        <thead>
          <tr>
            ${this.columns.map(col => html`<th>${col.label}</th>`)}
          </tr>
        </thead>
        <tbody>
          ${this.rows.map((row, index) => html`
            <tr @click="${() => this._handleRowClick(row, index)}">
              ${this.columns.map(col => html`<td>${row[col.key] ?? '-'}</td>`)}
            </tr>
          `)}
        </tbody>
      </table>
      ${this.totalCount > this.pageSize ? html`
        <div class="pagination">
          <span>共 ${this.totalCount} 条</span>
          <div>
            <button
              ?disabled="${this.page <= 1}"
              @click="${() => this._handlePageChange(this.page - 1)}"
            >上一页</button>
            <span style="margin: 0 8px">${this.page} / ${this.totalPages}</span>
            <button
              ?disabled="${this.page >= this.totalPages}"
              @click="${() => this._handlePageChange(this.page + 1)}"
            >下一页</button>
          </div>
        </div>
      ` : ''}
    `
  }
}
