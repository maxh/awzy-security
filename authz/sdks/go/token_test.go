package awzyauthz

import (
	"context"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"errors"
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v5"
)

func key(t *testing.T) *ecdsa.PrivateKey {
	t.Helper()
	k, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	return k
}

func sign(t *testing.T, k *ecdsa.PrivateKey, iss, aud string, claims jwt.MapClaims) string {
	t.Helper()
	all := jwt.MapClaims{"iss": iss, "aud": aud, "exp": time.Now().Add(5 * time.Minute).Unix()}
	for k, v := range claims {
		all[k] = v
	}
	s, err := jwt.NewWithClaims(jwt.SigningMethodES256, all).SignedString(k)
	if err != nil {
		t.Fatal(err)
	}
	return "Bearer " + s
}

func keyOf(k *ecdsa.PrivateKey) jwt.Keyfunc {
	return func(*jwt.Token) (any, error) { return &k.PublicKey, nil }
}

func TestAPlainTokenIsAPersonAndActIsAnAgentActingForThem(t *testing.T) {
	idp := key(t)
	v := NewTokenVerifier(TokenOptions{Issuer: "https://idp.test", Audience: "awzy", Keys: keyOf(idp)})
	p, err := v.Principal(sign(t, idp, "https://idp.test", "awzy", jwt.MapClaims{"sub": "alice"}))
	if err != nil || p.Principal != (EntityRef{"Human", "alice"}) || p.OnBehalfOf != nil {
		t.Fatalf("%+v %v", p, err)
	}
	p, err = v.Principal(sign(t, idp, "https://idp.test", "awzy", jwt.MapClaims{"sub": "alice", "act": map[string]any{"sub": "sales-genie"}, "awzy_human_present": false}))
	if err != nil || p.Principal != (EntityRef{"ServiceAccount", "sales-genie"}) || *p.OnBehalfOf != (EntityRef{"Human", "alice"}) || *p.HumanPresent {
		t.Fatalf("%+v %v", p, err)
	}
	p, err = v.Principal(sign(t, idp, "https://idp.test", "awzy", jwt.MapClaims{"sub": "alice", "act": map[string]any{"sub": "sales-genie"}, "awzy_work_item_id": "jira:SALES-12"}))
	if err != nil || p.WorkItemID != "jira:SALES-12" {
		t.Fatalf("the work item: %+v %v", p, err)
	}
}

func TestAnApplicationsOwnTokenIsKnownByItsClientCredentialsClaims(t *testing.T) {
	idp := key(t)
	v := NewTokenVerifier(TokenOptions{Issuer: "https://idp.test", Audience: "awzy", Keys: keyOf(idp), SubjectClaim: "email"})
	for _, c := range []jwt.MapClaims{{"sub": "nightly-sync", "client_id": "nightly-sync"}, {"sub": "0oa9app", "cid": "0oa9app"}, {"sub": "5f1c-app", "idtyp": "app"}} {
		p, err := v.Principal(sign(t, idp, "https://idp.test", "awzy", c))
		if err != nil || p.Principal.Type != "ServiceAccount" {
			t.Fatalf("%v: %+v %v", c, p, err)
		}
	}
	p, err := v.Principal(sign(t, idp, "https://idp.test", "awzy", jwt.MapClaims{"sub": "00u1", "email": "bob@acme.com"}))
	if err != nil || p.Principal != (EntityRef{"Human", "bob@acme.com"}) {
		t.Fatalf("%+v %v", p, err)
	}
	if _, err := v.Principal(sign(t, idp, "https://idp.test", "awzy", jwt.MapClaims{"sub": "00u1"})); !errors.Is(err, ErrToken) {
		t.Fatalf("no email: %v", err)
	}
}

func TestATokenForAnotherAudienceOrIssuerOrNoneIsRefused(t *testing.T) {
	idp := key(t)
	v := NewTokenVerifier(TokenOptions{Issuer: "https://idp.test", Audience: "awzy", Keys: keyOf(idp)})
	for _, bad := range []string{
		sign(t, idp, "https://idp.test", "other", jwt.MapClaims{"sub": "alice"}),
		sign(t, idp, "https://evil.test", "awzy", jwt.MapClaims{"sub": "alice"}),
		"Bearer x", "",
	} {
		if _, err := v.Principal(bad); !errors.Is(err, ErrToken) {
			t.Fatalf("%q: %v", bad, err)
		}
	}
}

func TestAnAppSignsOnlyForItsOwnAgents(t *testing.T) {
	idp, app := key(t), key(t)
	v := NewTokenVerifier(TokenOptions{Issuer: "https://idp.test", Audience: "awzy", Keys: keyOf(idp),
		TrustedApps: []TrustedApp{{Name: "acme-crm", Issuer: "https://crm.acme.example", Audience: "awzy-agents", Keys: keyOf(app)}}})
	crm := func(c jwt.MapClaims) string { return sign(t, app, "https://crm.acme.example", "awzy-agents", c) }
	p, err := v.Principal(crm(jwt.MapClaims{"sub": "acme-crm.renewals-bot"}))
	if err != nil || p.Principal != (EntityRef{"ServiceAccount", "acme-crm.renewals-bot"}) {
		t.Fatalf("%+v %v", p, err)
	}
	p, err = v.Principal(crm(jwt.MapClaims{"sub": "dana", "act": map[string]any{"sub": "acme-crm.assistant"}}))
	if err != nil || *p.OnBehalfOf != (EntityRef{"Human", "dana"}) {
		t.Fatalf("%+v %v", p, err)
	}
	for _, c := range []jwt.MapClaims{{"sub": "dana"}, {"sub": "sales-genie"}, {"sub": "acme-crm"}, {"sub": "x", "act": map[string]any{"sub": "bot"}}} {
		if _, err := v.Principal(crm(c)); !errors.Is(err, ErrToken) {
			t.Fatalf("%v: %v", c, err)
		}
	}
	if _, err := v.Principal(sign(t, app, "https://idp.test", "awzy", jwt.MapClaims{"sub": "acme-crm.bot"})); err == nil {
		t.Fatal("an app's key signing as the IdP")
	}
}

func TestAJWKSURLMustBeHTTPS(t *testing.T) {
	if _, err := JWKS(context.Background(), "http://idp.test/jwks", ""); !errors.Is(err, ErrToken) {
		t.Fatal(err)
	}
}
