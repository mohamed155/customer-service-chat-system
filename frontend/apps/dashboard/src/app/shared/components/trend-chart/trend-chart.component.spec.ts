import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { TrendChartComponent, TrendSeries } from './trend-chart.component';

function createComponent(series: TrendSeries[], labels: string[]) {
  TestBed.configureTestingModule({
    imports: [TrendChartComponent],
    providers: [provideZonelessChangeDetection()],
  });
  TestBed.compileComponents();
  const fixture = TestBed.createComponent(TrendChartComponent);
  fixture.componentRef.setInput('series', series);
  fixture.componentRef.setInput('labels', labels);
  fixture.detectChanges();
  return fixture;
}

describe('TrendChartComponent', () => {
  it('renders one polyline per series', () => {
    const series: TrendSeries[] = [{ id: 'a', label: 'A', color: 'chart-1', points: [1, 2, 3] }];
    const fixture = createComponent(series, ['d1', 'd2', 'd3']);
    const polylines = fixture.nativeElement.querySelectorAll('polyline');
    expect(polylines.length).toBe(1);
  });

  it('renders a legend when there are two series', () => {
    const series: TrendSeries[] = [
      { id: 'a', label: 'A', color: 'chart-1', points: [1, 2, 3] },
      { id: 'b', label: 'B', color: 'chart-2', points: [4, 5, 6] },
    ];
    const fixture = createComponent(series, ['d1', 'd2', 'd3']);
    const legend = fixture.nativeElement.querySelector('.legend');
    expect(legend).toBeTruthy();
    expect(legend.querySelectorAll('li').length).toBe(2);
  });

  it('does not render a legend with one series', () => {
    const series: TrendSeries[] = [{ id: 'a', label: 'A', color: 'chart-1', points: [1, 2, 3] }];
    const fixture = createComponent(series, ['d1', 'd2', 'd3']);
    expect(fixture.nativeElement.querySelector('.legend')).toBeNull();
  });

  it('renders one hidden table body row per label', () => {
    const series: TrendSeries[] = [{ id: 'a', label: 'A', color: 'chart-1', points: [1, 2, 3] }];
    const fixture = createComponent(series, ['d1', 'd2', 'd3']);
    const table = fixture.nativeElement.querySelector('table.sr-only');
    expect(table).toBeTruthy();
    const rows = table.querySelectorAll('tbody tr');
    expect(rows.length).toBe(3);
  });

  it('does not produce NaN in points attribute when null points present', () => {
    const series: TrendSeries[] = [{ id: 'a', label: 'A', color: 'chart-1', points: [1, null, 3] }];
    const fixture = createComponent(series, ['d1', 'd2', 'd3']);
    const polylines = fixture.nativeElement.querySelectorAll('polyline');
    for (const pl of polylines) {
      const pts = pl.getAttribute('points');
      expect(pts).not.toContain('NaN');
    }
  });

  it('does not produce NaN when all values are identical (flat series)', () => {
    const series: TrendSeries[] = [{ id: 'a', label: 'A', color: 'chart-1', points: [0, 0, 0, 0] }];
    const fixture = createComponent(series, ['d1', 'd2', 'd3', 'd4']);
    const polylines = fixture.nativeElement.querySelectorAll('polyline');
    expect(polylines.length).toBe(1);
    const pts = polylines[0].getAttribute('points');
    expect(pts).not.toContain('NaN');
  });
});
