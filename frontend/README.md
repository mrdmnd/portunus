The website for portunuspl.us

# Develop:
If you haven't produced a virtualenv yet, make one:

    python3 -m venv env

Ensure you are in the env

    . env/bin/activate

Ensure dependencies are up-to-date

    pip install -t lib -r requirements.txt --upgrade --no-cache-dir

The -t lib flag copies the libraries into a lib folder, which is uploaded to App
Engine during deployment. The -r requirements.txt flag tells pip to install
everything from that file.

If you haven't installed google cloud SDK, install it:
    curl https://sdk.cloud.google.com | bash


To deploy, run

    gcloud app deploy
