{{- /* A released chart pins each image's digest (image-digests.yaml, added by
tools/ci/publish-chart.sh), unless image.tag picks other images. */ -}}
{{- define "awzy.image" -}}
{{- $tag := .root.Values.image.tag | default .root.Chart.AppVersion -}}
{{- $digest := "" -}}
{{- if not .root.Values.image.tag -}}
{{- $digest = get (.root.Files.Get "image-digests.yaml" | fromYaml) .name | default "" -}}
{{- end -}}
{{- required "image.registry is required" .root.Values.image.registry -}}/{{ .name }}:{{ $tag }}{{ with $digest }}@{{ . }}{{ end }}
{{- end -}}

{{- /* A third-party image mirrored into the release registry and signed (ADR 0057); `ref` is a
<name>:<tag>@<digest> from deploy/release/third-party-images.txt. The digest is written out, since
Kyverno rejects an update to anything whose image has none. */ -}}
{{- define "awzy.thirdPartyImage" -}}
{{- required "image.registry is required" .root.Values.image.registry -}}/third-party/{{ .ref }}
{{- end -}}

{{- define "awzy.labels" -}}
app.kubernetes.io/name: {{ .name }}
app.kubernetes.io/part-of: awzy
app.kubernetes.io/version: {{ .root.Values.image.tag | default .root.Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .root.Release.Service }}
{{- end -}}

{{- define "awzy.selector" -}}
app.kubernetes.io/name: {{ .name }}
{{- end -}}

{{/* A Deployment plus ClusterIP Service on 8080 for one awzy service, rolled out without
downtime (ADR 0072): a new pod is ready before an old one goes, an old one drains before it stops,
and with more than one replica they spread across nodes and zones and leave one at a time. With
`autoscaling.enabled`, `replicas` is the minimum and a HorizontalPodAutoscaler sets the count on CPU. */}}
{{- define "awzy.service" -}}
{{- $root := .root -}}
{{- $autoscaling := and .values.autoscaling .values.autoscaling.enabled -}}
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  {{- if not $autoscaling }}
  {{- /* Left out when autoscaling, or every sync would undo the autoscaler. */}}
  replicas: {{ .values.replicas }}
  {{- end }}
  strategy:
    type: RollingUpdate
    rollingUpdate: { maxUnavailable: 0, maxSurge: 1 }
  selector:
    matchLabels: {{- include "awzy.selector" . | nindent 6 }}
  template:
    metadata:
      labels: {{- include "awzy.labels" . | nindent 8 }}
    spec:
      serviceAccountName: {{ .serviceAccount | default "awzy" }}
      # Covers the shutdown delay plus time to finish in-flight requests.
      terminationGracePeriodSeconds: {{ add $root.Values.shutdownDelaySeconds 20 }}
      {{- if gt (int .values.replicas) 1 }}
      topologySpreadConstraints:
        - maxSkew: 1
          topologyKey: kubernetes.io/hostname
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels: {{- include "awzy.selector" . | nindent 14 }}
        - maxSkew: 1
          topologyKey: topology.kubernetes.io/zone
          whenUnsatisfiable: ScheduleAnyway
          labelSelector:
            matchLabels: {{- include "awzy.selector" . | nindent 14 }}
      {{- end }}
      volumes:
        # The root filesystem is read-only; a little scratch space for what needs a temporary file,
        # such as a CPU profile (AWZY_PROFILING).
        - name: tmp
          emptyDir: { sizeLimit: 64Mi }
        {{- with .tokens }}
        # Service tokens this workload sends, one per service it calls (ADR 0055).
        - name: awzy-service-tokens
          projected:
            sources:
              {{- range . }}
              - serviceAccountToken: { audience: {{ . }}, expirationSeconds: 600, path: {{ . }} }
              {{- end }}
        {{- end }}
        {{- with .volumes }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
      {{- with .root.Values.image.pullSecret }}
      imagePullSecrets: [{ name: {{ . }} }]
      {{- end }}
      securityContext:
        runAsNonRoot: true
        # distroless's nonroot, by number: the image names its user, and the kubelet can't check
        # that a name isn't root, so it refuses to start the container.
        runAsUser: 65532
        runAsGroup: 65532
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: {{ .name }}
          {{- /* `image`: another binary's image, e.g. larkspur-drive runs awzy-synthetic's. */}}
          image: {{ include "awzy.image" (dict "root" $root "name" (.image | default .name)) }}
          {{- with .args }}
          args: {{ toJson . }}
          {{- end }}
          ports:
            - { name: http, containerPort: 8080 }
          env:
            - { name: AWZY_LOG_FORMAT, value: {{ $root.Values.logFormat | quote }} }
            # Not ready on SIGTERM, but serving this long while traffic moves elsewhere.
            - { name: AWZY_SHUTDOWN_DELAY_SECS, value: {{ $root.Values.shutdownDelaySeconds | quote }} }
            # The commit this chart was released from (appVersion, set by publish-chart.sh).
            - { name: AWZY_RELEASE_VERSION, value: {{ $root.Chart.AppVersion | quote }} }
            {{- with $root.Values.tracing.otlpEndpoint }}
            # OpenTelemetry traces to your collector (ADR 0097).
            - { name: OTEL_EXPORTER_OTLP_ENDPOINT, value: {{ . | quote }} }
            - { name: OTEL_TRACES_SAMPLER_ARG, value: {{ $root.Values.tracing.sampleRatio | quote }} }
            {{- end }}
            {{- with .env }}
            {{- toYaml . | nindent 12 }}
            {{- end }}
          volumeMounts:
            - { name: tmp, mountPath: /tmp }
            {{- if .tokens }}
            - { name: awzy-service-tokens, mountPath: /var/run/secrets/awzy/tokens, readOnly: true }
            {{- end }}
            {{- with .mounts }}
            {{- toYaml . | nindent 12 }}
            {{- end }}
          readinessProbe:
            httpGet: { path: /readyz, port: http }
          livenessProbe:
            httpGet: { path: /healthz, port: http }
          resources: {{- toYaml .values.resources | nindent 12 }}
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: { drop: [ALL] }
---
apiVersion: v1
kind: Service
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  selector: {{- include "awzy.selector" . | nindent 4 }}
  ports:
    - { name: http, port: 8080, targetPort: http }
{{- if gt (int .values.replicas) 1 }}
---
# A node drain takes one replica at a time.
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  maxUnavailable: 1
  selector:
    matchLabels: {{- include "awzy.selector" . | nindent 6 }}
{{- end }}
{{- if $autoscaling }}
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {{ .name }}
  namespace: {{ .namespace }}
  labels: {{- include "awzy.labels" . | nindent 4 }}
spec:
  scaleTargetRef: { apiVersion: apps/v1, kind: Deployment, name: {{ .name }} }
  minReplicas: {{ .values.replicas }}
  maxReplicas: {{ .values.autoscaling.maxReplicas }}
  metrics:
    - type: Resource
      resource:
        name: cpu
        # Average use as a percentage of the pod's CPU request.
        target: { type: Utilization, averageUtilization: {{ .values.autoscaling.cpuPercent }} }
{{- end }}
{{- end -}}

{{- define "awzy.demoIdpIssuer" -}}
https://{{ required "clientPortal.hostname is required for demoIdp" .Values.clientPortal.hostname }}/idp
{{- end -}}

{{/*
The model routing's environment (ADR 0114), for a service with a feature that uses models: the
connections the install can reach, the routing to start with, and the API keys connections name.
*/}}
{{- define "awzy.modelsEnv" -}}
{{- $m := .Values.models -}}
- { name: AWZY_LLM_CONNECTIONS, value: {{ $m.connections | default dict | toJson | quote }} }
- { name: AWZY_LLM_ROUTING, value: {{ $m.routing | default dict | toJson | quote }} }
{{- range $env, $ref := $m.apiKeySecrets }}
- name: {{ $env }}
  valueFrom: { secretKeyRef: { name: {{ $ref.secret | quote }}, key: {{ $ref.key | default "api-key" | quote }} } }
{{- end }}
{{- end }}
