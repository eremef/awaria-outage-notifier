class AwariaCard extends HTMLElement {
  // Config setter required by Home Assistant
  setConfig(config) {
    this._config = config || {};
  }

  // Home Assistant sets this when state changes
  set hass(hass) {
    this._hass = hass;
    this.render();
  }

  // Helper to map provider to nice name and icon
  getProviderMeta(source) {
    const s = (source || '').toLowerCase();
    if (s.includes('tauron_heat')) return { name: 'Tauron Ciepło', icon: '🔥', color: '#ff5722' };
    if (s.includes('tauron')) return { name: 'Tauron', icon: '⚡', color: '#ff9800' };
    if (s.includes('pge')) return { name: 'PGE', icon: '⚡', color: '#ffb74d' };
    if (s.includes('energa')) return { name: 'Energa', icon: '⚡', color: '#ffa726' };
    if (s.includes('enea')) return { name: 'Enea', icon: '⚡', color: '#fb8c00' };
    if (s.includes('stoen')) return { name: 'Stoen', icon: '⚡', color: '#f57c00' };
    if (s.includes('mpwik_wroclaw') || s.includes('mpwik_warszawa') || s.includes('aquanet') || s.includes('wodociagi') || s.includes('pwik') || s.includes('zwik') || s.includes('wmk')) {
      return { name: source.toUpperCase().replace('_', ' '), icon: '💧', color: '#2196f3' };
    }
    if (s.includes('veolia') || s.includes('fortum') || s.includes('gpec')) {
      return { name: source.toUpperCase().replace('_', ' '), icon: '🔥', color: '#f44336' };
    }
    if (s.includes('psg')) return { name: 'PSG Gaz', icon: '💨', color: '#4caf50' };
    return { name: source.toUpperCase(), icon: '⚠️', color: '#757575' };
  }

  render() {
    if (!this._hass) return;

    // Discover all sensor.awaria_* entities
    const states = this._hass.states;
    const awariaEntities = Object.keys(states).filter(key => key.startsWith('sensor.awaria_'));

    let allAlerts = [];

    awariaEntities.forEach(entityId => {
      const stateObj = states[entityId];
      if (stateObj && stateObj.attributes && stateObj.attributes.alerts) {
        let alerts = stateObj.attributes.alerts;
        if (typeof alerts === 'string') {
          try { alerts = JSON.parse(alerts); } catch (e) { alerts = []; }
        }
        if (Array.isArray(alerts)) {
          // Filter to show only local outages
          const localAlerts = alerts.filter(a => a.isLocal === true || a.is_local === true);
          allAlerts = allAlerts.concat(localAlerts);
        }
      }
    });

    // Deduplicate alerts by hash
    const uniqueAlerts = [];
    const seenHashes = new Set();
    allAlerts.forEach(alert => {
      const hash = alert.hash || alert.to_hash;
      if (hash && !seenHashes.has(hash)) {
        seenHashes.add(hash);
        uniqueAlerts.push(alert);
      }
    });

    // Render card
    if (!this._shadowRoot) {
      this.attachShadow({ mode: 'open' });
    }

    const css = `
      ha-card {
        padding: 16px;
        background: var(--card-background-color, #ffffff);
        color: var(--primary-text-color, #212121);
        border-radius: var(--ha-card-border-radius, 12px);
        box-shadow: var(--ha-card-box-shadow, none);
        border: var(--ha-card-border-width, 1px) solid var(--ha-card-border-color, var(--divider-color, #e0e0e0));
      }
      .card-header {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 1.25rem;
        font-weight: 500;
        margin-bottom: 16px;
        color: var(--primary-text-color);
      }
      .header-icon {
        font-size: 1.5rem;
      }
      .alerts-container {
        display: flex;
        flex-direction: column;
        gap: 12px;
      }
      .alert-card {
        padding: 12px;
        background: var(--secondary-background-color, #f5f5f5);
        border-left: 4px solid var(--alert-color, #757575);
        border-radius: 4px;
        font-size: 0.9rem;
        line-height: 1.4;
        transition: transform 0.2s ease, box-shadow 0.2s ease;
      }
      .alert-card:hover {
        transform: translateY(-2px);
        box-shadow: 0 4px 8px rgba(0,0,0,0.05);
      }
      .alert-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        font-weight: 600;
        margin-bottom: 4px;
        color: var(--primary-text-color);
      }
      .provider-tag {
        display: flex;
        align-items: center;
        gap: 4px;
      }
      .alert-dates {
        font-size: 0.8rem;
        color: var(--secondary-text-color, #757575);
        margin-bottom: 6px;
        font-weight: 500;
      }
      .alert-location {
        font-weight: 500;
        margin-bottom: 4px;
        color: var(--primary-text-color);
      }
      .alert-msg {
        color: var(--secondary-text-color, #757575);
      }
      .no-alerts {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
        padding: 24px;
        color: var(--secondary-text-color);
        font-weight: 500;
      }
      .no-alerts-icon {
        font-size: 1.5rem;
        color: var(--success-color, #4caf50);
      }
    `;

    let alertsHtml = '';
    if (uniqueAlerts.length === 0) {
      alertsHtml = `
        <div class="no-alerts">
          <span class="no-alerts-icon">✅</span>
          <span>Brak aktywnych awarii w Twojej okolicy</span>
        </div>
      `;
    } else {
      alertsHtml = `<div class="alerts-container">`;
      uniqueAlerts.forEach(alert => {
        const meta = this.getProviderMeta(alert.source);
        const startDate = alert.startDate || 'Brak danych';
        const endDate = alert.endDate || 'Brak danych';
        const location = alert.location || 'Brak lokalizacji';
        const message = alert.message || 'Brak szczegółów';

        const maxLen = 420;
        let messageHtml = '';
        if (message.length > maxLen) {
            const visible = message.substring(0, maxLen) + '...';
            // Note: Home Assistant custom cards don't have global toggleMessage readily available without registering it or handling shadow DOM events, 
            // but we can add inline script or just provide the truncated text. We will just render it as truncated for now, or add a simple inline handler.
            messageHtml = `<div class="alert-msg">💬 <span title="${message}">${visible}</span></div>`;
        } else {
            messageHtml = `<div class="alert-msg">💬 ${message}</div>`;
        }

        alertsHtml += `
          <div class="alert-card" style="--alert-color: ${meta.color}">
            <div class="alert-header">
              <span class="provider-tag">
                <span>${meta.icon}</span>
                <span>${meta.name}</span>
              </span>
            </div>
            <div class="alert-dates">📅 ${startDate} - ${endDate}</div>
            <div class="alert-location">📍 Miejscowość: ${location}</div>
            ${messageHtml}
          </div>
        `;
      });
      alertsHtml += `</div>`;
    }

    this._shadowRoot.innerHTML = `
      <style>${css}</style>
      <ha-card>
        <div class="card-header">
          <span class="header-icon">⚠️</span>
          <span>Monitor Awarii</span>
        </div>
        ${alertsHtml}
      </ha-card>
    `;
  }

  getCardSize() {
    return 2;
  }
}

customElements.define('awaria-card', AwariaCard);

// Register in Home Assistant custom card selector
window.customCards = window.customCards || [];
window.customCards.push({
  type: 'awaria-card',
  name: 'Awaria Outage Card',
  description: 'Displays current and upcoming power, water, heat, and gas outages dynamically.'
});
