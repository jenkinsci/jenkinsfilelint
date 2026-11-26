pipeline {
    agent any
    options {
        timeout(time: 1, unit: 'FOREVER')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
