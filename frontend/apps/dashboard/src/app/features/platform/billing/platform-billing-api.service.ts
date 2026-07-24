import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';
import { ApiResponse } from '../../../core/api/api.models';
import { ApiService } from '../../../core/api/api.service';
import {
  PlatformPlan,
  PlatformPlanWire,
  platformPlansFromWire,
  PlatformSubscriptionRow,
  PlatformSubscriptionRowWire,
  platformSubscriptionsFromWire,
} from '../../../core/api/tenant-api.models';

export interface CreatePlanRequest {
  readonly name: string;
  readonly monthlyPriceCents: number;
  readonly tokenAllowance?: number | null;
  readonly conversationAllowance?: number | null;
  readonly isDefault?: boolean;
}

export interface PutSubscriptionRequest {
  readonly planId: string;
  readonly status?: string;
}

@Injectable({ providedIn: 'root' })
export class PlatformBillingApiService {
  private readonly api = inject(ApiService);

  getPlans(): Observable<ApiResponse<PlatformPlan[]>> {
    return this.api
      .get<{ data: PlatformPlanWire[] }>('/platform/billing/plans')
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: platformPlansFromWire(data.data) })));
  }

  createPlan(req: CreatePlanRequest): Observable<ApiResponse<PlatformPlan>> {
    const body = {
      name: req.name,
      monthly_price_cents: req.monthlyPriceCents,
      ...(req.tokenAllowance !== undefined ? { token_allowance: req.tokenAllowance } : {}),
      ...(req.conversationAllowance !== undefined
        ? { conversation_allowance: req.conversationAllowance }
        : {}),
      ...(req.isDefault !== undefined ? { is_default: req.isDefault } : {}),
    };
    return this.api
      .post<{ data: PlatformPlanWire }>('/platform/billing/plans', body)
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: platformPlansFromWire([data.data])[0] })));
  }

  updatePlanStatus(planId: string, status: string): Observable<ApiResponse<PlatformPlan>> {
    return this.api
      .patch<{ data: PlatformPlanWire }>(`/platform/billing/plans/${planId}/status`, { status })
      .pipe(map(({ data, ...rest }) => ({ ...rest, data: platformPlansFromWire([data.data])[0] })));
  }

  getSubscriptions(): Observable<ApiResponse<PlatformSubscriptionRow[]>> {
    return this.api
      .get<{ data: PlatformSubscriptionRowWire[] }>('/platform/billing/subscriptions')
      .pipe(
        map(({ data, ...rest }) => ({ ...rest, data: platformSubscriptionsFromWire(data.data) })),
      );
  }

  putSubscription(
    tenantId: string,
    req: PutSubscriptionRequest,
  ): Observable<ApiResponse<{ tenantId: string; planId: string; status: string }>> {
    const body = {
      plan_id: req.planId,
      ...(req.status ? { status: req.status } : {}),
    };
    return this.api
      .put<{ data: { tenant_id: string; plan_id: string; status: string } }>(
        `/platform/billing/subscriptions/${tenantId}`,
        body,
      )
      .pipe(
        map(({ data, ...rest }) => ({
          ...rest,
          data: {
            tenantId: data.data.tenant_id,
            planId: data.data.plan_id,
            status: data.data.status,
          },
        })),
      );
  }
}
