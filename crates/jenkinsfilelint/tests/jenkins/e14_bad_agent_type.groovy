pipeline {
    agent { foo 'x' }
    stages {
        stage('S') { steps { echo 'x' } }
    }
}
