/**
 * Follow intent for the story feed, kept separate from the scroll offset.
 *
 * Programmatic pins and the browser's own scrollTop clamping must never be
 * mistaken for reader movement. Only an observed upward move or a wheel-up
 * gesture releases follow; returning to the floor adopts it again.
 */
export class StoryScrollFollow {
  private following: boolean;
  private programmatic = false;
  private released = false;
  private lastTop = 0;

  constructor(initial = true, private readonly threshold = 4) {
    this.following = initial;
  }

  get active(): boolean {
    return this.following;
  }

  reset(following = true): void {
    this.following = following;
    this.lastTop = 0;
    this.programmatic = false;
    this.released = false;
  }

  setFollowing(active: boolean): void {
    this.following = active;
    this.released = !active;
  }

  wheelUp(): void {
    this.following = false;
    this.released = true;
  }

  beginProgrammatic(): void {
    this.programmatic = true;
  }

  endProgrammatic(top: number): void {
    this.programmatic = false;
    this.lastTop = top;
  }

  sample(top: number, floor: number): boolean {
    if (this.programmatic) return this.following;
    if (this.released) {
      this.following = Math.abs(floor - top) <= 1;
      if (this.following) this.released = false;
    } else if (top < this.lastTop - 1) {
      this.following = false;
      this.released = true;
    } else if (Math.abs(floor - top) <= this.threshold) {
      this.following = true;
    }
    this.lastTop = top;
    return this.following;
  }

  nearBottom(top: number, floor: number): boolean {
    return Math.abs(floor - top) <= this.threshold;
  }
}
