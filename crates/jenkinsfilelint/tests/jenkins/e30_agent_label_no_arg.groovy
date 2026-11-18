pipeline {
    agent { label }
    stages {
        stage('S') { steps { echo 'x' } }
    }
}
