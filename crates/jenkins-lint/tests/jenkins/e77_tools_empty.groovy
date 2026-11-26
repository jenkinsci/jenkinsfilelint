pipeline {
    agent any
    tools {
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
