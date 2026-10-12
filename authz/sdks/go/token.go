package awzyauthz

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"errors"
	"fmt"
	"net/http"
	"os"
	"strings"

	"github.com/MicahParks/keyfunc/v3"
	"github.com/golang-jwt/jwt/v5"
)

// ErrToken wraps every reason a caller's token isn't valid.
var ErrToken = errors.New("invalid token")

// ErrNoChallenge is returned by Wait on a result that isn't a challenge.
var ErrNoChallenge = errors.New("not a challenge")

// TrustedApp is an app that signs tokens for the service accounts whose ID starts with its name
// and a dot.
type TrustedApp struct {
	Name     string
	Issuer   string
	Audience string
	// Keys finds the key a token was signed with; see JWKS.
	Keys jwt.Keyfunc
}

// TokenOptions configure a TokenVerifier.
type TokenOptions struct {
	// Issuer is the client's IdP: the iss its tokens carry.
	Issuer string
	// Audience is the aud tokens for this app must carry.
	Audience string
	// Keys finds the key a token was signed with; see JWKS.
	Keys jwt.Keyfunc
	// SubjectClaim names the claim that identifies a person: their user name as the IdP
	// provisions them. Default sub; with Okta or Entra usually email.
	SubjectClaim string
	// ServiceAccounts lists service accounts whose own tokens carry no client-credentials marker.
	ServiceAccounts []string
	// TrustedApps are apps an admin trusts to sign tokens for their own agents.
	TrustedApps []TrustedApp
}

// JWKS fetches signing keys from a JWKS URL, which must be https: keys fetched over plain HTTP
// could be swapped in transit and used to forge tokens. caFile adds a private CA to trust.
func JWKS(ctx context.Context, url, caFile string) (jwt.Keyfunc, error) {
	if !strings.HasPrefix(url, "https://") {
		return nil, fmt.Errorf("%w: a JWKS URL must be https://, not %s", ErrToken, url)
	}
	client := http.DefaultClient
	if caFile != "" {
		pem, err := os.ReadFile(caFile)
		if err != nil {
			return nil, err
		}
		pool, err := x509.SystemCertPool()
		if err != nil {
			pool = x509.NewCertPool()
		}
		if !pool.AppendCertsFromPEM(pem) {
			return nil, fmt.Errorf("%s has no certificates", caFile)
		}
		client = &http.Client{Transport: &http.Transport{TLSClientConfig: &tls.Config{RootCAs: pool, MinVersion: tls.VersionTLS12}}}
	}
	k, err := keyfunc.NewDefaultOverrideCtx(ctx, []string{url}, keyfunc.Override{Client: client})
	if err != nil {
		return nil, err
	}
	return k.Keyfunc, nil
}

// TokenVerifier validates callers' tokens (signature, issuer, audience, expiry) and builds the
// principal from their claims only, as awzy-gateway does (ADR 0045, rule 1).
type TokenVerifier struct {
	opts            TokenOptions
	serviceAccounts map[string]bool
	apps            map[string]TrustedApp
}

func NewTokenVerifier(opts TokenOptions) *TokenVerifier {
	v := &TokenVerifier{opts: opts, serviceAccounts: map[string]bool{}, apps: map[string]TrustedApp{}}
	for _, id := range opts.ServiceAccounts {
		v.serviceAccounts[id] = true
	}
	for _, a := range opts.TrustedApps {
		v.apps[a.Issuer] = a
	}
	return v
}

func tokenErr(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrToken, fmt.Sprintf(format, args...))
}

// Principal returns the principal an Authorization header's bearer token names.
func (v *TokenVerifier) Principal(authorization string) (Principal, error) {
	token, ok := strings.CutPrefix(authorization, "Bearer ")
	if !ok || token == "" {
		return Principal{}, tokenErr("missing bearer token")
	}
	unverified, _, err := jwt.NewParser().ParseUnverified(token, jwt.MapClaims{})
	if err != nil {
		return Principal{}, tokenErr("not a JWT")
	}
	iss, _ := unverified.Claims.GetIssuer()
	if app, ok := v.apps[iss]; ok && iss != v.opts.Issuer {
		claims, err := verify(token, app.Keys, app.Issuer, app.Audience)
		if err != nil {
			return Principal{}, err
		}
		return appPrincipal(claims, app)
	}
	claims, err := verify(token, v.opts.Keys, v.opts.Issuer, v.opts.Audience)
	if err != nil {
		return Principal{}, err
	}
	sub, _ := claims["sub"].(string)
	if act := actSub(claims); act != "" {
		human, err := v.human(claims)
		if err != nil {
			return Principal{}, err
		}
		return withAwzyClaims(claims, Principal{Principal: EntityRef{"ServiceAccount", act}, OnBehalfOf: &EntityRef{"Human", human}}), nil
	}
	if isClientCredentials(claims) || v.serviceAccounts[sub] {
		return withAwzyClaims(claims, Principal{Principal: EntityRef{"ServiceAccount", sub}}), nil
	}
	human, err := v.human(claims)
	if err != nil {
		return Principal{}, err
	}
	return withAwzyClaims(claims, Principal{Principal: EntityRef{"Human", human}}), nil
}

func verify(token string, keys jwt.Keyfunc, issuer, audience string) (jwt.MapClaims, error) {
	claims := jwt.MapClaims{}
	_, err := jwt.ParseWithClaims(token, claims, keys,
		jwt.WithValidMethods([]string{"RS256", "ES256"}),
		jwt.WithIssuer(issuer),
		jwt.WithAudience(audience),
		jwt.WithExpirationRequired(),
	)
	if err != nil {
		return nil, tokenErr("%v", err)
	}
	if sub, _ := claims["sub"].(string); sub == "" {
		return nil, tokenErr("the token has no sub")
	}
	return claims, nil
}

func actSub(claims jwt.MapClaims) string {
	act, _ := claims["act"].(map[string]any)
	sub, _ := act["sub"].(string)
	return sub
}

// isClientCredentials: a token an application got for itself, by RFC 9068's client_id, Okta's
// cid, or Entra's idtyp.
func isClientCredentials(claims jwt.MapClaims) bool {
	sub, _ := claims["sub"].(string)
	clientID, _ := claims["client_id"].(string)
	cid, _ := claims["cid"].(string)
	idtyp, _ := claims["idtyp"].(string)
	return clientID == sub || cid == sub || idtyp == "app"
}

func (v *TokenVerifier) human(claims jwt.MapClaims) (string, error) {
	claim := v.opts.SubjectClaim
	if claim == "" || claim == "sub" {
		sub, _ := claims["sub"].(string)
		return sub, nil
	}
	id, _ := claims[claim].(string)
	if id == "" {
		return "", tokenErr("the token has no %q claim to identify the person", claim)
	}
	return id, nil
}

// appPrincipal: an app signs only for its own agents, on their own or acting for a person,
// never for a person on their own.
func appPrincipal(claims jwt.MapClaims, app TrustedApp) (Principal, error) {
	sub, _ := claims["sub"].(string)
	act := actSub(claims)
	agent := sub
	if act != "" {
		agent = act
	}
	prefix := app.Name + "."
	if !strings.HasPrefix(agent, prefix) || agent == prefix {
		return Principal{}, tokenErr("%s signs tokens only for service accounts under %q, not %q", app.Name, prefix, agent)
	}
	p := Principal{Principal: EntityRef{"ServiceAccount", agent}}
	if act != "" {
		p.OnBehalfOf = &EntityRef{"Human", sub}
	}
	return withAwzyClaims(claims, p), nil
}

// withAwzyClaims adds what awzy's own claims say: whether a person is present, and the work item.
func withAwzyClaims(claims jwt.MapClaims, p Principal) Principal {
	if present, ok := claims["awzy_human_present"].(bool); ok {
		p.HumanPresent = &present
	}
	if item, ok := claims["awzy_work_item_id"].(string); ok {
		p.WorkItemID = strings.TrimSpace(item)
	}
	return p
}
