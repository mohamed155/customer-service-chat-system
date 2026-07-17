import { TestBed } from '@angular/core/testing';
import { IndexStatusBadgeComponent } from './index-status-badge.component';
import { IndexStatus } from '../../../core/api/knowledge.models';

describe('IndexStatusBadgeComponent', () => {
  function createFixture(status: IndexStatus) {
    TestBed.configureTestingModule({ imports: [IndexStatusBadgeComponent] });
    const fixture = TestBed.createComponent(IndexStatusBadgeComponent);
    fixture.componentRef.setInput('indexStatus', status);
    fixture.detectChanges();
    return fixture;
  }

  it('renders not_indexed', () => {
    const fixture = createFixture({ status: 'not_indexed', chunkCount: 0 });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('not_indexed');
    expect(host.textContent).toContain('Not Indexed');
  });

  it('renders pending with spinner', () => {
    const fixture = createFixture({ status: 'pending', chunkCount: 0 });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('pending');
    expect(host.querySelector('.spinner')).toBeTruthy();
    expect(host.textContent).toContain('Pending');
  });

  it('renders indexing with spinner', () => {
    const fixture = createFixture({ status: 'indexing', chunkCount: 0 });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('indexing');
    expect(host.querySelector('.spinner')).toBeTruthy();
    expect(host.textContent).toContain('Indexing');
  });

  it('renders indexed with chunk count', () => {
    const fixture = createFixture({ status: 'indexed', chunkCount: 15 });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('indexed');
    expect(host.textContent).toContain('Indexed');
    expect(host.textContent).toContain('(15)');
  });

  it('renders failed', () => {
    const fixture = createFixture({
      status: 'failed',
      chunkCount: 0,
      failureReason: 'Provider error',
    });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('failed');
    expect(host.textContent).toContain('Failed');
  });

  it('renders not_indexable', () => {
    const fixture = createFixture({
      status: 'not_indexable',
      chunkCount: 0,
      failureReason: 'No extractable text',
    });
    const host = fixture.nativeElement as HTMLElement;
    expect(host.classList).toContain('not_indexable');
    expect(host.textContent).toContain('Not Indexable');
  });
});
