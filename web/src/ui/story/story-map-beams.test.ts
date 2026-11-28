import { describe, expect, it } from 'vitest';
import { MapBeams } from './story-map-beams.js';

describe('MapBeams', () => {
  it('每个技能卡画一条连线，且重复调度不会丢线', async () => {
    const shell = document.createElement('div');
    shell.className = 'map-shell';
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    for (let index = 0; index < 3; index += 1) {
      const lane = document.createElement('section');
      lane.className = 'map-lane';
      const card = document.createElement('article');
      card.className = 'map-card map-card--skill';
      lane.appendChild(card);
      shell.appendChild(lane);
    }
    shell.appendChild(svg);

    const beams = new MapBeams(shell, svg);
    beams.schedule();
    beams.schedule();
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(svg.querySelectorAll('path')).toHaveLength(3);
    beams.dispose();
  });
});
