import { TestBed } from '@angular/core/testing';
import { MfaService } from './mfa.service';
import { StepUpService } from './step-up.service';

describe('StepUpService', () => {
  let service: StepUpService;
  let mfa: { stepUp: ReturnType<typeof vi.fn> };

  beforeEach(() => {
    mfa = { stepUp: vi.fn() };
    TestBed.configureTestingModule({ providers: [{ provide: MfaService, useValue: mfa }] });
    service = TestBed.inject(StepUpService);
  });

  it('opens the prompt and resolves true once a code is proven', async () => {
    const answer = service.prompt();
    expect(service.open()).toBe(true);
    mfa.stepUp.mockResolvedValue(undefined);
    await service.submit({ code: '123456' });
    await expect(answer).resolves.toBe(true);
    expect(service.open()).toBe(false);
    expect(service.errorKey()).toBeNull();
  });

  it('resolves false when the user cancels', async () => {
    const answer = service.prompt();
    service.cancel();
    await expect(answer).resolves.toBe(false);
    expect(service.open()).toBe(false);
  });

  it('keeps the prompt open and says why when the code is wrong', async () => {
    const answer = service.prompt();
    mfa.stepUp.mockRejectedValue({ code: 'x', message: 'x', status: 422 });
    await service.submit({ code: '000000' });
    expect(service.open()).toBe(true);
    expect(service.errorKey()).toBe('errors.codeInvalid');
    expect(service.pending()).toBe(false);
    service.cancel();
    await expect(answer).resolves.toBe(false);
  });

  it('gives requests that arrive while it is open the same answer', async () => {
    const first = service.prompt();
    const second = service.prompt();
    mfa.stepUp.mockResolvedValue(undefined);
    await service.submit({ code: '123456' });
    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(mfa.stepUp).toHaveBeenCalledTimes(1);
  });
});
