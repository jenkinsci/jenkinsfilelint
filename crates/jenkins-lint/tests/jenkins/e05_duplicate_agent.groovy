pipeline {
    agent any
    agent none
    stages {
        stage('S') { steps { echo 'x' } }
    }
}
