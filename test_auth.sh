#!/bin/bash

# Load .env file
if [ -f .env ]; then
  export $(grep -v '^#' .env | xargs)
fi

echo "Testing Authentication..."
echo "URL: $SPLUNK_BASE_URL"
# echo "Token: $SPLUNK_TOKEN" # Don't print secret

echo "----------------------------------------"
echo "Method 1: JWT (Authorization: Bearer <token>)"
curl -k -s -o response_jwt.json -w "%{http_code}" -X GET \
  "$SPLUNK_BASE_URL/services/authentication/current-context?output_mode=json" \
  -H "Authorization: Bearer $SPLUNK_TOKEN" > status_jwt.txt

HTTP_CODE=$(cat status_jwt.txt)
echo "HTTP Status: $HTTP_CODE"
if [ "$HTTP_CODE" == "200" ]; then
    echo "SUCCESS!"
    cat response_jwt.json | grep "username"
else
    echo "FAILED."
    cat response_jwt.json
fi
rm status_jwt.txt response_jwt.json

echo "----------------------------------------"
echo "Method 2: Session Key (Authorization: Splunk <token>)"
# Only useful if Method 1 fails and it's actually a session key, but user has a JWT now.
# We'll run it just in case, but expect 401 if it's a JWT.
curl -k -s -o response_session.json -w "%{http_code}" -X GET \
  "$SPLUNK_BASE_URL/services/authentication/current-context?output_mode=json" \
  -H "Authorization: Splunk $SPLUNK_TOKEN" > status_session.txt

HTTP_CODE=$(cat status_session.txt)
echo "HTTP Status: $HTTP_CODE"
if [ "$HTTP_CODE" == "200" ]; then
    echo "SUCCESS!"
    cat response_session.json | grep "username"
else
    echo "FAILED (Expected if using JWT)."
    # cat response_session.json
fi
rm status_session.txt response_session.json
echo "----------------------------------------"
