import { TestBed } from '@angular/core/testing';
import { areaAccessGuard } from './area-access.guard';

describe('areaAccessGuard', () => {
  it('passes through while authentication is not implemented', () => {
    const result = TestBed.runInInjectionContext(() =>
      areaAccessGuard({ path: 'tenant' }, [], {} as never),
    );
    expect(result).toBe(true);
  });
});
