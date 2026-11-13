import { Routes } from '@angular/router';
import { marker } from '../../core/i18n/marker';
import { APP_PATHS } from '../../core/router/app-paths';

export const AUTH_ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: APP_PATHS.auth.login },
  {
    path: APP_PATHS.auth.login,
    loadComponent: () => import('./login/login.component').then((m) => m.LoginComponent),
    title: marker('titles.signIn'),
  },
  {
    path: APP_PATHS.auth.mfa,
    loadComponent: () =>
      import('./mfa-challenge/mfa-challenge.component').then((m) => m.MfaChallengeComponent),
    title: marker('titles.mfaChallenge'),
  },
  {
    path: APP_PATHS.auth.signup,
    loadComponent: () => import('./signup/signup.component').then((m) => m.SignupComponent),
    title: marker('titles.signup'),
  },
  {
    path: APP_PATHS.auth.forgotPassword,
    loadComponent: () =>
      import('./forgot-password/forgot-password.component').then((m) => m.ForgotPasswordComponent),
    title: marker('titles.forgotPassword'),
  },
  {
    path: APP_PATHS.auth.verifyEmail,
    loadComponent: () =>
      import('./verify-email/verify-email.component').then((m) => m.VerifyEmailComponent),
    title: marker('titles.verifyEmail'),
  },
];
