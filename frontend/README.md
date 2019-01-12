The website for portunuspl.us

# Develop:
Ensure you are in the venv
    . venv/bin/activate

Ensure dependencies are up-to-date
    pip install -t lib -r requirements.txt --upgrade

The -t lib flag copies the libraries into a lib folder, which is uploaded to App
Engine during deployment. The -r requirements.txt flag tells pip to install
everything from that file.

To deploy, run
    gcloud app deploy
