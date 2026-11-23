import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { TestBed } from '@angular/core/testing';
import { firstValueFrom } from 'rxjs';
import { APP_CONFIG } from '../config/app-config';
import { authTokenInterceptor } from '../http/auth-token.interceptor';
import { PasswordResetService } from './password-reset.service';

describe('PasswordResetService', () => {
  let http: HttpTestingController;
  let service: PasswordResetService;
  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(withInterceptors([authTokenInterceptor])),
        provideHttpClientTesting(),
        { provide: APP_CONFIG, useValue: { apiBaseUrl: '/api/v1' } },
      ],
    });
    service = TestBed.inject(PasswordResetService);
    http = TestBed.inject(HttpTestingController);
  });
  afterEach(() => http.verify());

  it('posts just the email with the request proof', async () => {
    const done = firstValueFrom(service.forgot('user@example.com'));
    const req = http.expectOne({ method: 'POST', url: '/api/v1/auth/password/forgot' });
    expect(req.request.body).toEqual({ email: 'user@example.com' });
    expect(req.request.headers.get('X-Requested-With')).toBe('XMLHttpRequest');
    req.flush(null, { status: 202, statusText: 'Accepted' });
    await done;
  });

  it('posts the token and password in the body and accepts a 204', async () => {
    const done = firstValueFrom(service.reset('sample-token', 'new password'));
    const req = http.expectOne({ method: 'POST', url: '/api/v1/auth/password/reset' });
    expect(req.request.body).toEqual({ token: 'sample-token', password: 'new password' });
    expect(req.request.headers.get('X-Requested-With')).toBe('XMLHttpRequest');
    req.flush(null, { status: 204, statusText: 'No Content' });
    await done;
  });
});
