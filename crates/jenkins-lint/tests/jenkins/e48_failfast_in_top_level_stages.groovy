pipeline {
    agent any
    stages {
        failFast true
        stage('A') { steps { echo 'x' } }
    }
}
