import { ComponentFixture, TestBed } from '@angular/core/testing';
import { HealthIndicatorComponent } from './health-indicator.component';

describe('HealthIndicatorComponent', () => {
  async function createComponent(
    verdict: 'healthy' | 'degraded' | 'unhealthy',
  ): Promise<ComponentFixture<HealthIndicatorComponent>> {
    await TestBed.configureTestingModule({
      imports: [HealthIndicatorComponent],
    }).compileComponents();
    const fixture = TestBed.createComponent(HealthIndicatorComponent);
    fixture.componentRef.setInput('verdict', verdict);
    fixture.detectChanges();
    return fixture;
  }

  it('renders Healthy label for healthy verdict', async () => {
    const fixture = await createComponent('healthy');
    expect(fixture.nativeElement.textContent).toContain('Healthy');
  });

  it('renders Degraded label for degraded verdict', async () => {
    const fixture = await createComponent('degraded');
    expect(fixture.nativeElement.textContent).toContain('Degraded');
  });

  it('renders Unhealthy label for unhealthy verdict', async () => {
    const fixture = await createComponent('unhealthy');
    expect(fixture.nativeElement.textContent).toContain('Unhealthy');
  });
});
