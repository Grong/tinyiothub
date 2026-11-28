import { describe, expect, it } from 'vitest';
import { StoryScrollFollow } from './story-scroll.js';

describe('StoryScrollFollow', () => {
  it('releases follow on a wheel-up and does not re-adopt it near the floor', () => {
    const follow = new StoryScrollFollow(true);
    follow.sample(500, 500);
    follow.wheelUp();
    follow.sample(496, 500);
    expect(follow.active).toBe(false);
    follow.sample(498, 500);
    expect(follow.active).toBe(false);
    follow.sample(500, 500);
    expect(follow.active).toBe(true);
  });

  it('ignores programmatic scroll samples', () => {
    const follow = new StoryScrollFollow(true);
    follow.beginProgrammatic();
    follow.sample(0, 1000);
    expect(follow.active).toBe(true);
    follow.endProgrammatic(1000);
  });
});
